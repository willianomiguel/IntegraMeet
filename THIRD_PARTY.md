# Componentes de terceiros

O IntegraMeet usa os componentes abaixo. Cada um mantém a sua própria licença.

| Componente | Uso | Licença | Origem |
|---|---|---|---|
| whisper.cpp (whisper-cli, build CPU, Windows x64) | Transcrição | MIT | https://github.com/ggml-org/whisper.cpp |
| llama.cpp (llama-server, build Vulkan, Windows x64) | Análise com IA local | MIT | https://github.com/ggml-org/llama.cpp |
| OpenMP (libomp.dll) | Dependência do llama.cpp | Apache 2.0 com exceções LLVM | `src-tauri/resources/runtime/llama/LICENSE-LLVM-OpenMP` |
| Modelos Whisper (ggml-base, ggml-small, ggml-medium) | Transcrição | MIT | https://huggingface.co/ggerganov/whisper.cpp |
| Modelo Qwen2.5-7B-Instruct (GGUF Q4_K_M) | Análise com IA (perfil Alta qualidade) | Apache 2.0 | https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF |
| Modelo Qwen2.5-3B-Instruct (GGUF Q4_K_M) | Análise com IA (perfis Leve e Balanceado) | Qwen Research License (conferir os termos na página) | https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF |
| Tauri, React, Tailwind, rusqlite, reqwest e demais crates/pacotes | Base do aplicativo | MIT / Apache 2.0 | Ver `package.json` e `src-tauri/Cargo.toml` |

Os modelos são baixados pelo próprio aplicativo na primeira configuração, diretamente das páginas oficiais acima, e são conferidos por SHA-256 antes de serem usados.
