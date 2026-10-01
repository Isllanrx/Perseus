# Glossario

- **client_id** — token publico do player web (32 alfanumericos) embutido nos bundles JS; nao identifica usuario.
- **stub** — faixa em playlist que vem so com `id`; precisa de `/tracks?ids=` para metadados e transcodings.
- **transcoding** — variante de stream (protocolo + mime). `progressive` = arquivo unico; `hls` = segmentos; `*-encrypted-hls` = DRM.
- **snipped** — previa de 30s (conteudo Go+).
- **track_authorization** — token por faixa exigido ao pedir a URL do stream.
- **settled** — faixa que o watcher nao precisa mais tentar (concluida, indisponivel ou abandonada).
- **Reporter / Event** — publicador de eventos tipados de uma execucao: cada evento vira log `tracing` e e entregue ao adaptador (CLI ou janela).
- **job** — execucao iniciada pela interface (download ou monitoramento), com eventos numerados e replay via `job_events`.
- **part** — arquivo `<nome>.part` em escrita; so vira o arquivo final apos validacao.
- **run_id** — id de correlacao de uma execucao, presente em todos os logs e no relatorio.
- **Archive**: `.perseus-archive.json` de cada pasta; mapeia track id -> arquivo.
- **Biblioteca**: `library.json` global; permite copiar uma faixa ja baixada para outra pasta sem rede.
- **Colecao**: `/albums` ou `/sets` de um perfil; varias playlists, cada uma na sua pasta.
- **Playlist virtual**: likes, uploads, populares, reposts ou relacionadas tratadas como playlist.
- **Sync**: mover para `Removed/` o que saiu da playlist.
- **Contrato**: arquivo em `contracts/` que Rust e front leem nos testes; muda os dois lados juntos.
- **Retry de transporte**: repeticao feita pela camada HTTP (5xx, 429, timeout) antes de a faixa ver o erro;
  **retry da faixa** refaz a transferencia inteira com stream novo. Contados separados nas stats.
- **RawServer**: servidor HTTP de teste em TCP cru para falhas que o wiremock nao simula.
- **Mutante**: alteracao proposital do codigo (cargo-mutants/Stryker); **equivalente** quando nao muda o resultado
  e nenhum teste pode distingui-lo.
- **Sanity / Full**: perfis do `test-all.ps1` (rapido apos mudanca / com fuzz, desempenho, mutacao e flakiness).
