//! Catálogo fixo de modelos. Cada arquivo tem tamanho e SHA-256 conferidos
//! nos repositórios oficiais (Hugging Face, LFS oid). Trocar um modelo exige
//! atualizar esta lista e publicar uma nova versão do app.

use serde::Serialize;

use crate::hardware::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Transcription,
    Intelligence,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub name: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    pub id: &'static str,
    pub kind: ModelKind,
    pub title: &'static str,
    pub purpose: &'static str,
    pub requirement: &'static str,
    pub files: &'static [ModelFile],
}

impl ModelSpec {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

const WHISPER_BASE: ModelSpec = ModelSpec {
    id: "whisper-base",
    kind: ModelKind::Transcription,
    title: "Transcrição rápida",
    purpose: "Transcreve a reunião em português com menor uso de memória.",
    requirement: "Qualquer computador com Windows 10 ou 11.",
    files: &[ModelFile {
        name: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    }],
};

const WHISPER_SMALL: ModelSpec = ModelSpec {
    id: "whisper-small",
    kind: ModelKind::Transcription,
    title: "Transcrição equilibrada",
    purpose: "Boa precisão em português com consumo moderado.",
    requirement: "8 GB de memória RAM recomendados.",
    files: &[ModelFile {
        name: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    }],
};

const WHISPER_MEDIUM: ModelSpec = ModelSpec {
    id: "whisper-medium",
    kind: ModelKind::Transcription,
    title: "Transcrição de alta precisão",
    purpose: "Máxima precisão para reuniões com sotaques e termos técnicos.",
    requirement: "16 GB de memória RAM recomendados.",
    files: &[ModelFile {
        name: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
    }],
};

const QWEN_3B: ModelSpec = ModelSpec {
    id: "qwen2.5-3b",
    kind: ModelKind::Intelligence,
    title: "Inteligência local",
    purpose: "Gera resumo, decisões, tarefas e prazos a partir da transcrição.",
    requirement: "8 GB de memória RAM recomendados.",
    files: &[ModelFile {
        name: "qwen2.5-3b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
        size: 2_104_932_768,
        sha256: "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d",
    }],
};

const QWEN_7B: ModelSpec = ModelSpec {
    id: "qwen2.5-7b",
    kind: ModelKind::Intelligence,
    title: "Inteligência de alta qualidade",
    purpose: "Análises mais completas e fiéis ao texto da reunião.",
    requirement: "16 GB de memória RAM e 8 GB de VRAM recomendados.",
    files: &[
        ModelFile {
            name: "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
            url: "https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF/resolve/main/qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
            size: 3_993_201_344,
            sha256: "dfce12e3862a5283ccfb88221b48480e58745165de856439950d0f22590580db",
        },
        ModelFile {
            name: "qwen2.5-7b-instruct-q4_k_m-00002-of-00002.gguf",
            url: "https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF/resolve/main/qwen2.5-7b-instruct-q4_k_m-00002-of-00002.gguf",
            size: 689_872_288,
            sha256: "539cf93f78e887edea1c04e2d7d8cdaca9d01dae9c9025bcb8accbe29df3d72a",
        },
    ],
};

pub const ALL: &[ModelSpec] = &[WHISPER_BASE, WHISPER_SMALL, WHISPER_MEDIUM, QWEN_3B, QWEN_7B];

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    ALL.iter().find(|m| m.id == id)
}

/// Modelos que o perfil recomenda baixar no onboarding.
pub fn for_profile(profile: Profile) -> (&'static str, &'static str) {
    match profile {
        Profile::Leve => ("whisper-base", "qwen2.5-3b"),
        Profile::Balanceado => ("whisper-small", "qwen2.5-3b"),
        Profile::Alta => ("whisper-medium", "qwen2.5-7b"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_has_valid_sha256_and_size() {
        for m in ALL {
            for f in m.files {
                assert_eq!(f.sha256.len(), 64, "hash inválido em {}", f.name);
                assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                assert!(f.size > 0);
                assert!(f.url.starts_with("https://huggingface.co/"));
            }
        }
    }

    #[test]
    fn profiles_point_to_existing_models() {
        for p in [Profile::Leve, Profile::Balanceado, Profile::Alta] {
            let (t, i) = for_profile(p);
            assert!(find(t).is_some(), "transcrição ausente: {t}");
            assert!(find(i).is_some(), "IA ausente: {i}");
        }
    }
}
