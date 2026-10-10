// Gera a release do instalador: release/IntegraMeet-Setup.exe + SHA256SUMS.txt.
// Uso: npm run release
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { createHash } from "node:crypto";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const bundleDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const releaseDir = join(root, "release");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

// Data de build exposta em "Sobre" via variável de ambiente lida em compile time.
process.env.LMAI_BUILD_DATE = new Date().toISOString().slice(0, 10);

// O rustup instala o cargo em ~/.cargo/bin, que nem sempre está no PATH do terminal.
const cargoBin = join(process.env.USERPROFILE ?? process.env.HOME ?? "", ".cargo", "bin");
process.env.PATH = `${cargoBin}${process.platform === "win32" ? ";" : ":"}${process.env.PATH}`;

console.log(`> Gerando build ${pkg.version} (${process.env.LMAI_BUILD_DATE})`);
execSync("npx tauri build", { cwd: root, stdio: "inherit", env: process.env });

// Escolhe o instalador da versão atual (a pasta do bundle pode ter builds antigos).
const built = readdirSync(bundleDir).find((f) => f.startsWith(`IntegraMeet_${pkg.version}_`) && f.endsWith("-setup.exe"));
if (!built) throw new Error(`Instalador não encontrado em ${bundleDir}`);

rmSync(releaseDir, { recursive: true, force: true });
mkdirSync(releaseDir, { recursive: true });
const setupName = "IntegraMeet-Setup.exe";
const dest = join(releaseDir, setupName);
copyFileSync(join(bundleDir, built), dest);

const sha256 = createHash("sha256").update(readFileSync(dest)).digest("hex");
writeFileSync(join(releaseDir, "SHA256SUMS.txt"), `${sha256}  ${setupName}\n`);

const sizeMb = (statSync(dest).size / 1024 / 1024).toFixed(1);
writeFileSync(
  join(releaseDir, "release-info.json"),
  JSON.stringify(
    { version: pkg.version, buildDate: process.env.LMAI_BUILD_DATE, file: setupName, sizeMb: Number(sizeMb), sha256, platform: "Windows 10/11 x64" },
    null,
    2,
  ) + "\n",
);

console.log(`\nRelease pronta: release/${setupName} (${sizeMb} MB)`);
console.log(`SHA-256: ${sha256}`);
if (!existsSync(dest)) throw new Error("Falha ao copiar o instalador");
