# Seguranca

## Invariantes
- URLs de entrada: somente `soundcloud.com` / `m.` / `www.` e shortlinks `on.soundcloud.com`; sem credenciais nem porta; path restrito a `[A-Za-z0-9_-]`; so `in` e `secret_token` sobrevivem, e ambos sao validados por regex. O parse e manual: o parser WHATWG normalizaria `..`/percent-encoding antes da validacao.
- Bytes so sao baixados de `https://` em hosts `*.sndcdn.com`, `*.soundcloud.com`, `*.soundcloud.cloud` (`Http::ensure_trusted`) — vale para stream, cada segmento HLS, init map, artwork e cada redirecionamento (politica de redirect customizada; o cliente e `https_only`).
- Nunca usar streams `*-encrypted-hls` nem HLS com `#EXT-X-KEY` diferente de `NONE` (anti-circumvention). Previas `snipped` sao recusadas. HLS com byte-range e recusado (evita arquivo corrompido).
- Paginacao: `next_href` so e seguido se comecar com `api_base`; limites de paginas, playlists por colecao, relacionadas e resultados de busca. Query de busca 1..=200 chars; URLs dos resultados passam por `normalize_url`.
- Resume por Range: so aceita `206` com `Content-Range` iniciando no offset pedido; caso contrario recomeca do zero.
- Archive por pasta: entradas cujo `file` nao e nome simples (separador, `:`, `..`, NUL) sao descartadas na carga, senao reaproveitamento/sync poderiam apagar ou mover fora da pasta. Biblioteca global (`%LOCALAPPDATA%`) so e lida: a origem e revalidada como audio do mesmo formato antes da copia.
- Limites: audio 1 GiB, artwork 10 MiB, manifesto 5 MiB, 10k segmentos, respostas da API 32 MiB, pagina/bundle JS 16 MiB, URL 2048 chars, `Retry-After` limitado a 60 s.
- Nomes de arquivo: sanitizacao (separadores, controles, nomes reservados Windows, `..`, NFC) + `ensure_within(base, target)` (normalizacao lexica + `canonicalize` quando existe, contra symlinks).
- Escrita atomica: `.part` + `rename`, removido por guard (`PartFile`) em falha ou cancelamento; cache do client_id via `tempfile` + `persist` em `%LOCALAPPDATA%\Perseus\cache`, fora do repositorio.
- client_id mascarado nos logs; erros de rede nunca carregam a URL (assinatura da CDN/client_id). Sem cookie store.
- App desktop: sem servidor HTTP nem porta. IPC restrito a janela `main` por capabilities (so eventos expostos alem dos comandos do app); CSP em `tauri.conf.json`; `open_folder` abre apenas o `target_dir` que o backend registrou para o job; `JobIn` com `deny_unknown_fields` e limites validados no Rust.
- `HostPolicy::Loopback` (aceita `http://127.0.0.1`) existe so em `cfg(test)`/feature `test-util`.
- Versao web: a funcao nunca transporta audio; `/api/stream` so aceita URL de transcoding (regex do path,
  sem query, host confiavel) e `track_authorization` `[0-9A-Za-z._-]{1,4096}`; `/api/plan` com `deny_unknown_fields`,
  ate 1000 faixas; corpo ate 16 KiB; limite por IP (150 req/min por instancia, 429 + `Retry-After`, mesma cota da regra
  do Firewall da Vercel); o front espaca as chamadas com intervalo aleatorio (`pacer.ts`); sem CORS (so a
  propria origem); `plan`/`stream`/erros com `no-store`. No navegador, `isTrustedMediaUrl` repete a allowlist de
  midia e a CSP do `vercel.json` limita `connect-src`/`img-src` aos hosts do SoundCloud. HSTS sem `preload` (seria
  aplicado ao dominio inteiro `isllan.dev`).
- Release: actions fixadas por SHA, `cargo deny` (RustSec, licencas, origens), `npm audit`, SHA256SUMS e atestado de proveniencia.

## Historico
- v0.1: shortlink expandido com `"on.soundcloud.com" in url` (aceitava `evil.com/?on.soundcloud.com`); `?in=` injetado sem validacao; fallback `transcodings[0]` podia escolher stream criptografado; cache gravado dentro do pacote; client_id completo em log INFO. Todos corrigidos na v0.2.
- v0.2 (Python): superficie HTTP local defendida com token por sessao, TrustedHost, checagem de Origin e CSP. Eliminada na v1.0 (reescrita em Rust): nao ha mais API HTTP.
- v1.0: redirects de CDN passaram a ser validados contra a allowlist (antes o `requests` seguia qualquer host).
- v1.0 (catalogo e biblioteca): archive adulterado com `..\` podia direcionar remocao/movimentacao para fora da pasta; corrigido antes do release com validacao na carga e teste.
- v1.0 (testes de propriedade): nomes de arquivo aceitavam controles C1 (U+0080-U+009F) e formatacao bidirecional (U+202E) vindos de titulos; um titulo podia exibir extensao falsa no Explorer. `INVALID_CHARS` passou a cobrir `\p{Cc}` e bidi.
