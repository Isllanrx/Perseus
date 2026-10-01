# Historico da v0.2 (Python)

**Situacao:** Substituidas pela [reescrita em Rust e Tauri](reescrita-rust-tauri.md) (2026-09-30). Mantidas como
registro de por que a v0.2 era assim e do que a v1.0 deixou de precisar.

## Engine Scrapy em subprocesso com failover para engine direto

### Contexto
O projeto nasceu com Scrapy. O reactor do Twisted nao reinicia no mesmo processo (modo watch roda varios
ciclos), a GUI precisa cancelar downloads de verdade e o pipeline v0.1 fazia download bloqueante dentro do
reactor, serializando tudo. Era requisito ter failover caso o engine principal caisse.

### Decisao
- Scrapy roda em subprocesso `multiprocessing` com contexto `spawn`; logs voltam ao pai por
  `QueueHandler`/`QueueListener` e o relatorio por `Queue`. Cancelar = `terminate()`.
- Downloads de audio saem do reactor: `ThreadPool` dedicado do Twisted via `deferToThreadPool`.
- Engine direto (`requests` + `ThreadPoolExecutor`) compartilha planejamento, fetcher e validacao.
- `--engine auto`: se o Scrapy falhar por **infraestrutura** (`engine_failure`: crash, import, reactor), o
  direto assume. Erro de dominio (404, recurso nao suportado) nao dispara failover — falharia de novo.

### Alternativas consideradas
- **Crawl4AI como segundo engine**: crawler de paginas com Chromium; os dados vem de API JSON, entao
  adicionaria ~300 MB de browser e superficie de ataque sem ganho. Rejeitado.
- **So o engine direto**: mais simples, mas perderia AutoThrottle/stats do Scrapy.
- **CrawlerRunner no mesmo processo**: sem cancelamento real e sujeito a `ReactorNotRestartable`.

### Consequencias
- Custo de spawn (~1-2 s) por execucao; aceitavel.
- No exe, `freeze_support()` precisa ser a primeira chamada do launcher.
- Failover e idempotente porque arquivos integros sao reaproveitados.

## Interface web local (FastAPI + React) no lugar do Tkinter

### Contexto
A GUI Tkinter tinha problemas estruturais (widgets alterados fora da thread principal, botao "Parar" que nao
parava nada, logs do subprocesso invisiveis) e o objetivo era uma interface React moderna, responsiva e com a
identidade visual Perseus.

### Decisao
- Servidor FastAPI/uvicorn **so em 127.0.0.1**, porta aleatoria, token por sessao injetado no `index.html`.
- Defesas: `TrustedHostMiddleware` (DNS rebinding), checagem de `Origin` (CSRF), CSP restrita, sem OpenAPI.
- Progresso por SSE nativo do FastAPI, gerado a partir dos logs estruturados (`JobEventHandler`), com replay
  por `Last-Event-ID`.
- Front-end React 19 + Vite, fontes empacotadas localmente (sem Google Fonts). Tkinter removido para nao manter
  duas GUIs.

### Alternativas consideradas
- **Electron/Tauri**: janela nativa, mas runtime extra (Electron) ou toolchain Rust (Tauri) e empacotamento
  mais complexo junto ao backend Python.
- **pywebview**: depende de WebView2 e complica testes E2E.
- **Manter Tkinter corrigido**: nao entregava uma UI React.

### Consequencias
- UI testavel com Playwright contra o servidor real (SoundCloud simulado).
- Depende de navegador instalado; o exe abre a URL local automaticamente.
- Build do React fora do git; precisa estar presente no wheel/exe (CI garante).

## Executavel PyInstaller onedir com console visivel

### Contexto
O `.exe` nao podia gerar falso positivo no Windows Defender/SmartScreen. Executaveis PyInstaller sao
frequentemente marcados por heuristica, principalmente no modo onefile.

### Decisao
- **onedir** (pasta com `Perseus.exe` + `_internal\`): onefile se autoextrai em `%TEMP%` e executa de la,
  padrao classico de dropper que heuristicas marcam.
- **Sem UPX**: executavel comprimido/empacotado e sinal forte de malware.
- **Metadados de versao** (empresa, produto, copyright, versao) e icone proprio: binario sem metadados pesa
  contra na reputacao.
- **Console visivel**: um servidor local rodando escondido e abrindo porta e exatamente o comportamento que AV
  marca; com console o usuario ve a URL e encerra fechando a janela.
- Manifesto `asInvoker` (sem pedir administrador).
- Distribuicao com `SHA256SUMS` e build provenance attestation no release.

### Alternativas consideradas
- **onefile**: mais pratico de distribuir, mas maior taxa de falso positivo.
- **Nuitka**: compila para C, tambem sofre falsos positivos e exige compilador no build.
- **Bootloader PyInstaller compilado localmente**: reduz assinaturas conhecidas, mas exige MSVC no build.

### Consequencias
- A mitigacao definitiva e **assinatura Authenticode** com certificado de code signing (backlog #9).
- Falsos positivos residuais: enviar para analise em https://www.microsoft.com/en-us/wdsi/filesubmission.
