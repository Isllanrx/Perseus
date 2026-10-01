# Versao web na Vercel: servidor so de metadados, audio direto do CDN ao navegador

**Situacao:** Em vigor (2026-09-30). Complementa a [reescrita em Rust e Tauri](reescrita-rust-tauri.md) (o app desktop
nao muda).

## Contexto
Objetivo: oferecer o Perseus online (dominio proprio `isllan.dev`, conta Vercel) para quem nao quer instalar,
responsivo em desktop, Android e iOS, sem afetar o app desktop. Fatos medidos em 2026-09-30:
- `api-v2.soundcloud.com` nao envia `Access-Control-Allow-Origin`: o navegador nao consegue falar com a API.
- `cf-media.sndcdn.com`, `cf-hls-media.sndcdn.com`, `playback.media-streaming.soundcloud.cloud` e `i1.sndcdn.com`
  respondem com `Access-Control-Allow-Origin: *` (progressive, manifestos, segmentos e capas).
- Funcoes da Vercel tem limite de 4,5 MB por resposta nao-streaming, cobram banda de origem e tem duracao limitada.
- O runtime Rust da Vercel (beta publico, `vercel_runtime` 2.4) compila `api/*.rs` com `cargo build --bin <nome>`
  na raiz do workspace; validado em `amazonlinux:2023` (imagem base do build): 1m16s, 9,6 MB, sem cmake.

## Decisao
- **Servidor so de metadados** (`crates/perseus-web`, Axum; adaptador `api/perseus.rs` com `VercelLayer`): `config`,
  `inspect`, `search`, `plan` e `stream`, todos JSON pequeno. Reaproveita o `perseus-core` inteiro: allowlist de
  URLs, shortlinks, client_id, escolha de stream sem DRM, nomes, pastas e numeracao.
- **Planejamento sem disco no core** (`features::download::remote::plan_remote`): o `planning.rs` foi separado em
  parte pura (`folders`, `Selection`, `claim_unique_name`, `playlist_file_name`) e parte de disco; `plan_download` e
  o engine usam a mesma parte pura, entao desktop e web aplicam exatamente as mesmas regras.
- **Audio direto do CDN no navegador** (`web/src/lib/web`): `/api/stream` devolve a URL assinada (progressive) ou as
  partes do HLS ja validadas pelo core; a aba baixa, valida, grava tags (ID3v2.3 em MP3, `ilst` no MP4 fragmentado)
  e entrega. Retry por faixa e por segmento com a mesma politica do core.
- **Mesmo contrato do IPC**: o backend web implementa os comandos e eventos (`JobEvent`) do `src-tauri`; a UI React
  e a mesma, escolhida em tempo de build (`vite --mode web`). O ramo nao usado sai do bundle (verificado: o bundle
  desktop nao contem nenhum codigo web).
- **Destino**: pasta escolhida pelo usuario via File System Access (Chromium desktop; escrita atomica pelo
  `createWritable`) ou downloads do navegador (arquivo unico; playlists num `.zip` sem compressao via `client-zip`),
  o que funciona no Android e no iOS.
- **Sem monitoramento, biblioteca, sync e limite de banda na web**: exigem processo sempre ligado ou disco
  persistente. O plano web aceita ate 1000 faixas (resposta abaixo do limite da funcao); acima disso, erro traduzido
  pedindo limite ou o app desktop.
- **Responsivo** com regras escopadas em `:root[data-platform="web"]` (atributo que so o HTML do build web recebe):
  safe-area do iOS, `100dvh`, alvos de toque de 44 px, sem zoom ao focar campos, sem "puxar para atualizar".

## Alternativas consideradas
- *Servidor baixa e devolve o audio*: esbarra no limite de 4,5 MB e na duracao da funcao, e paga banda por
  download. Descartado.
- *Reescrever o backend em TypeScript (Node na Vercel)*: duplicaria allowlist, descoberta de client_id, escolha de
  stream e nomes, com risco de divergir do desktop. Descartado em favor do core em Rust.
- *Modo standalone da Vercel (servidor Axum atendendo tudo)*: o servidor tambem serviria os estaticos, que deixariam
  o CDN. Descartado; o modo classico `api/*.rs` + rewrite mantem os estaticos no CDN.
- *lofty compilado para WASM no navegador*: paridade exata de tags, mas exigiria Rust + wasm-bindgen no build web.
  Os escritores em TS foram conferidos com o lofty: mesmos campos do arquivo gerado pelo desktop (M4A do HLS AAC
  identico em tags; a web grava tambem o `LABEL`, que o lofty nao mapeia de `Publisher` para MP4).

## Consequencias
- Nenhum byte de audio passa pela Vercel; o custo e so de invocacoes com JSON pequeno.
- Todos os usuarios compartilham o client_id e o IP de saida da funcao. Tres camadas de ritmo:
  (1) no navegador, `Pacer` compartilhado pela aba espaca `plan`/`stream` com intervalo aleatorio de 400-900 ms
  (~92/min por visitante, sem rajadas com padrao de robo); (2) na funcao, 150 req/min por IP e por instancia (429 +
  `Retry-After`, que o motor respeita); (3) regra de rate limit do Firewall da Vercel com a mesma cota (unica regra
  gratuita do Hobby; configurada no painel, o `vercel.json` nao aceita rate limit). Um `SOUNDCLOUD_CLIENT_ID` fixo
  evita a descoberta em instancia fria. Bloqueio pelo SoundCloud do IP da Vercel e um risco aceito.
- **Plano Hobby (gratuito, sem cobranca de excedente; cota esgotada pausa o recurso por ate 30 dias)**: `maxDuration`
  30 s (memoria provisionada fixa de 2 GB conta pelo tempo em execucao), gzip na origem (Fast Origin Transfer),
  `config` com `s-maxage=86400` e `inspect`/`search` com `s-maxage=120` (o CDN responde sem invocar a funcao), regiao
  unica padrao, sem recursos pagos (Blob, KV, Cron, Image Optimization).
- Ogg Opus sai sem tags na web (so aparece quando e o unico formato da faixa).
- iOS: o manifest usa `display: browser`; apps de tela inicial em modo standalone nao entregam downloads de forma
  confiavel.
