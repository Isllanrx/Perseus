# Roadmap

## Visao
Perseus e o downloader open source mais robusto para conteudo **publico** do SoundCloud, sem conta de usuario:
correto por padrao (nunca DRM, nunca previa disfarcada de faixa completa), resiliente a mudancas da API,
observavel e facil de usar tanto por quem nao programa (instalador + app) quanto por automacao (CLI + JSON).

## Principios que guiam as escolhas
- Legitimidade antes de cobertura: so streams abertos; faixas indisponiveis sao explicadas, nao contornadas.
- Idempotencia: rodar de novo nunca rebaixa o que ja esta integro.
- Falha explicavel: todo erro chega ao usuario com causa e acao sugerida; todo evento tem `run_id`.
- Distribuicao confiavel: artefatos verificaveis (checksum, provenance) e, quando possivel, assinados.

## Horizontes
### Entregue (v0.2)
Refatoracao VSA, UI React, testes, exe Windows (Python; substituido na v1.0).

### Agora (v1.0 — Rust + Tauri)
Core assincrono em Rust, HLS paralelo, app Tauri 2 com IPC, CLI nativa, instalador NSIS, 11 idiomas; catalogo
completo sem login (perfis, curtidas, albuns, relacionadas, busca), qualidade Melhor, templates, archive,
biblioteca, sync nao destrutivo, `.m3u8`, retomada por Range; suite de testes por categoria com CI em 3 motores.
(O arquivo original do artista foi descartado: exige login.)

### Proximo (v1.1 — o que o SoundCloud 2026 oferece e ninguem usa; backlog 15-26)
- Estacoes e trending (`/discover/sets/...`), playlists curtidas, navegador de charts por regiao.
- Preservacao (faixas da biblioteca que sumiram do SoundCloud), dedupe por ISRC, radar de artistas.
- Busca avancada (Creative Commons, facetas), completar album a partir de um single.

### Depois (v1.2 — distribuicao)
- Assinatura Authenticode e atualizacao automatica (`tauri-plugin-updater`).
- Remux opcional com ffmpeg (MP4 fragmentado, conversao de formato).
- Builds macOS/Linux do app.

### Futuro
- i18n na CLI (a UI ja tem 11 idiomas).
- Persistencia opcional do historico de jobs (hoje em memoria).
- `perseus-core` como crate publica para uso programatico.
