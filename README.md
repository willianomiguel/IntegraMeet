# IntegraMeet

**Assistente privado para reuniões.** Grave, transcreva e organize suas reuniões com inteligência artificial **direto no seu computador**. Nada é enviado para a internet.

> Projeto de código aberto, sem fins lucrativos, feito para ajudar as pessoas.
> Criado por **willian_sdm** — https://www.instagram.com/willian_sdm/

---

## ⬇️ Baixar

Baixe o instalador na página de releases:

**[➜ Baixar IntegraMeet para Windows](https://github.com/willianomiguel/IntegraMeet/releases/latest/download/IntegraMeet-Setup.exe)**

Todas as versões ficam em [Releases](https://github.com/willianomiguel/IntegraMeet/releases). Cada release traz um arquivo `SHA256SUMS.txt` para você conferir se o download chegou inteiro.

---

## O que o IntegraMeet faz

- **Grava** a sua reunião pelo microfone e, se quiser, também o áudio do computador (a voz das outras pessoas).
- **Transcreve** a gravação em texto, com o tempo de cada trecho.
- **Resume** a reunião e destaca **decisões, tarefas, prazos, responsáveis, pendências, riscos e perguntas**.
- **Reproduz o áudio** a partir de qualquer trecho da transcrição.
- **Esconde sons de ambiente** (barulho, música, risos) se você quiser, e **copia a transcrição** com um clique.
- **Pesquisa** em todas as reuniões, pelo título e pelo que foi dito.

## Por que é privado

- Todo o processamento acontece **no seu computador**.
- Não há telemetria, conta, cadastro nem envio de áudio, texto ou resumo para serviços externos.
- A internet só é usada para **baixar os modelos de IA** na primeira configuração e para **baixar atualizações** do programa. Depois disso, ele funciona offline.

## Como funciona

1. **Baixe e instale** o arquivo `IntegraMeet-Setup.exe`.
2. **Abra o programa.** Ele mostra uma configuração inicial em português.
3. **Verificação do computador:** o app analisa o seu computador e escolhe sozinho o melhor desempenho (Leve, Balanceado ou Alta qualidade). Você não precisa saber nada técnico.
4. **Modelos:** o app baixa os modelos de transcrição e de inteligência. Eles ficam salvos no seu computador.
5. **Áudio:** você escolhe o microfone e testa se ele está funcionando.
6. **Teste rápido:** grava 5 segundos, toca de volta e transcreve, para confirmar que está tudo certo.
7. **Pronto:** clique em **Nova reunião**, dê um nome, inicie a gravação e finalize quando terminar. A transcrição e a análise começam sozinhas.

## Requisitos

| | Mínimo | Recomendado |
|---|---|---|
| Sistema | Windows 10 (versão 2004) ou Windows 11, 64 bits | Windows 11 |
| Memória RAM | 8 GB | 16 GB ou mais |
| Placa de vídeo | Não é obrigatória (usa o processador) | Placa com suporte a Vulkan (NVIDIA, AMD ou Intel), 6 GB de VRAM ou mais |
| Espaço em disco | Cerca de 6 GB para os modelos e as reuniões | 20 GB ou mais |

## Aviso ao instalar

O instalador ainda **não tem assinatura digital**, então o Windows pode mostrar "O Windows protegeu o computador". Para continuar, clique em **Mais informações** e depois em **Executar assim mesmo**. Você pode conferir a integridade do arquivo com o `SHA256SUMS.txt` da release.

---

## Para desenvolvedores

Tecnologias: [Tauri 2](https://tauri.app) (Rust), React, TypeScript, Tailwind, SQLite (FTS5), [whisper.cpp](https://github.com/ggml-org/whisper.cpp) e [llama.cpp](https://github.com/ggml-org/llama.cpp).

Pré-requisitos: Node.js 22+, Rust (via [rustup](https://rustup.rs)) e as ferramentas de build do Visual Studio com o componente C++.

```bash
npm install
npm run tauri dev        # desenvolvimento
npm test                 # testes do frontend
cargo test --manifest-path src-tauri/Cargo.toml   # testes do backend
npm run release          # gera release/IntegraMeet-Setup.exe
```

Testes que baixam modelos e usam o whisper e o llama reais ficam marcados como `ignore`. Para rodar:

```bash
cargo test pipeline_real -- --ignored --nocapture
```

Estrutura principal:

- `src/` — interface (telas de configuração, gravação, reuniões e pesquisa)
- `src-tauri/src/` — núcleo: captura WASAPI, gravação, transcrição, análise, banco, modelos e processos locais
- `src-tauri/resources/runtime/` — whisper-cli e llama-server (builds oficiais, com licenças incluídas)
- `src-tauri/windows/hooks.nsi` — escolhas do desinstalador (manter ou apagar reuniões e modelos)
- `site/` — página de download

## Privacidade e dados

- Gravações, transcrições e análises ficam em `Documentos\IntegraMeet` (ou na pasta que você escolher).
- Configurações e banco de dados ficam em `%APPDATA%\LocalMeetingAI`. Modelos e cache ficam em `%LOCALAPPDATA%\LocalMeetingAI`.
- O desinstalador **mantém suas reuniões e modelos por padrão**. Você decide se quer apagá-los.

## Contribuir

Contribuições são bem-vindas. Abra uma *issue* para relatar problemas ou sugerir melhorias, ou envie um *pull request*.

## Licença

Código sob a [licença MIT](LICENSE). Componentes de terceiros e seus modelos mantêm as próprias licenças, listadas em [THIRD_PARTY.md](THIRD_PARTY.md).
