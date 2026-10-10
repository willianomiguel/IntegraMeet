//! Verificação do computador: Windows, CPU, RAM, GPU/VRAM, espaço em disco.
//!
//! A GPU é detectada pelo próprio `llama-server --list-devices` (build Vulkan
//! embutido), então não precisa de consulta extra ao driver.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
    pub vram_free_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub os_caption: String,
    pub os_build: u32,
    pub windows_supported: bool,
    pub cpu_name: String,
    pub cpu_threads: usize,
    pub ram_gb: f64,
    pub gpus: Vec<GpuInfo>,
    pub disk_free_gb: f64,
    pub vulkan_available: bool,
}

/// Perfil recomendado. O usuário final vê só o nome; os modelos são escolhidos aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Leve,
    Balanceado,
    Alta,
}

const MIN_BUILD_WIN10: u32 = 19041;

pub fn detect(llama_dir: &Path, data_dir: &Path) -> HardwareInfo {
    let sys = query_windows_info();
    let gpus = list_vulkan_devices(llama_dir);
    let disk_free_gb = fs2::available_space(data_dir).unwrap_or(0) as f64 / 1_073_741_824.0;

    HardwareInfo {
        os_caption: sys.os_caption.clone(),
        os_build: sys.os_build,
        windows_supported: cfg!(windows) && sys.os_build >= MIN_BUILD_WIN10,
        cpu_name: sys.cpu_name,
        cpu_threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
        ram_gb: sys.ram_bytes as f64 / 1_073_741_824.0,
        vulkan_available: !gpus.is_empty(),
        gpus,
        disk_free_gb,
    }
}

/// Escolhe o perfil com base na memória e na VRAM disponíveis.
pub fn recommend(hw: &HardwareInfo) -> Profile {
    let vram_gb = hw.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0) as f64 / 1024.0;
    if hw.ram_gb >= 16.0 && vram_gb >= 8.0 {
        Profile::Alta
    } else if hw.ram_gb >= 8.0 {
        Profile::Balanceado
    } else {
        Profile::Leve
    }
}

struct WindowsInfo {
    os_caption: String,
    os_build: u32,
    cpu_name: String,
    ram_bytes: u64,
}

fn query_windows_info() -> WindowsInfo {
    // Uma única chamada ao PowerShell devolve JSON com tudo que precisamos.
    let script = "$os = Get-CimInstance Win32_OperatingSystem; \
                  $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1; \
                  [pscustomobject]@{ caption = $os.Caption; build = [int]$os.BuildNumber; \
                  cpu = $cpu.Name.Trim(); ram = [int64]$os.TotalVisibleMemorySize * 1024 } | ConvertTo-Json -Compress";

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Raw {
        caption: String,
        build: u32,
        cpu: String,
        ram: u64,
    }

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let parsed = cmd
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<Raw>(&o.stdout).ok());

    match parsed {
        Some(raw) => WindowsInfo {
            os_caption: raw.caption,
            os_build: raw.build,
            cpu_name: raw.cpu,
            ram_bytes: raw.ram,
        },
        None => WindowsInfo {
            os_caption: "Windows (não identificado)".into(),
            os_build: 0,
            cpu_name: "Processador (não identificado)".into(),
            ram_bytes: 0,
        },
    }
}

/// Lê linhas como `  Vulkan0: NVIDIA GeForce GTX 1660 (6180 MiB, 5403 MiB free)`.
fn list_vulkan_devices(llama_dir: &Path) -> Vec<GpuInfo> {
    let exe = llama_dir.join("llama-server.exe");
    if !exe.exists() {
        return vec![];
    }
    let mut cmd = Command::new(&exe);
    cmd.arg("--list-devices");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let Ok(out) = cmd.output() else { return vec![] };
    let text = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);

    text.lines()
        .filter_map(parse_device_line)
        .collect()
}

fn parse_device_line(line: &str) -> Option<GpuInfo> {
    let line = line.trim();
    if !line.starts_with("Vulkan") {
        return None;
    }
    let after_colon = line.split_once(':')?.1.trim();
    let (name, rest) = after_colon.rsplit_once(" (")?;
    let nums = rest.trim_end_matches(')');
    let mut parts = nums.split(',').map(|p| p.trim());
    let total = parts.next()?.split_whitespace().next()?.parse().ok()?;
    let free = parts
        .next()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(GpuInfo {
        name: name.trim().to_string(),
        vram_mb: total,
        vram_free_mb: free,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vulkan_device_line() {
        let g = parse_device_line("  Vulkan0: NVIDIA GeForce GTX 1660 (6180 MiB, 5403 MiB free)")
            .expect("linha válida");
        assert_eq!(g.name, "NVIDIA GeForce GTX 1660");
        assert_eq!(g.vram_mb, 6180);
        assert_eq!(g.vram_free_mb, 5403);
    }

    #[test]
    fn ignores_non_device_lines() {
        assert!(parse_device_line("Available devices:").is_none());
    }

    #[test]
    fn recommends_balanced_for_reference_machine() {
        let hw = HardwareInfo {
            os_caption: String::new(),
            os_build: 26100,
            windows_supported: true,
            cpu_name: String::new(),
            cpu_threads: 16,
            ram_gb: 32.0,
            gpus: vec![GpuInfo { name: "GTX 1660".into(), vram_mb: 6180, vram_free_mb: 5000 }],
            disk_free_gb: 100.0,
            vulkan_available: true,
        };
        assert_eq!(recommend(&hw), Profile::Balanceado);
    }

    #[test]
    fn recommends_light_for_low_memory() {
        let hw = HardwareInfo {
            os_caption: String::new(),
            os_build: 19045,
            windows_supported: true,
            cpu_name: String::new(),
            cpu_threads: 4,
            ram_gb: 4.0,
            gpus: vec![],
            disk_free_gb: 50.0,
            vulkan_available: false,
        };
        assert_eq!(recommend(&hw), Profile::Leve);
    }
}
