# Catalogo completo do perfil, biblioteca local e sync nao destrutivo

**Situacao:** Em vigor (2026-09-30)

## Contexto
Comparativo com scdl, yt-dlp, DownCloud, imthaghost/scdl, SoundCloudExplode e NotTobi/soundcloud-dl mostrou tres
lacunas: (1) perfis, curtidas, reposts, populares, albuns e relacionadas nao eram suportados; (2) a idempotencia
dependia do nome do arquivo, entao trocar o template ou a ordem rebaixava faixas; (3) nao havia sync, busca nem
escolha de qualidade. Nenhuma das ferramentas reaproveita faixas entre pastas, retoma por Range com verificacao,
checa espaco em disco ou sincroniza sem apagar.

## Decisao
- Abas do perfil viram `PlaylistSource` (Likes, Uploads, PopularTracks, Reposts, Related) e `/albums` e `/sets`
  viram `Resource::Collection`, com uma pasta por playlist. Paginacao generica por `next_href`, aceito so se
  comecar com `api_base` (nao segue host arbitrario).
- Archive por pasta (`.perseus-archive.json`, por track id, com arquivo e qualidade) e biblioteca global
  (`library.json` em `data_local_dir`). A biblioteca so copia arquivo valido do disco e reescreve as tags; entradas
  cujo arquivo sumiu sao descartadas ao salvar.
- Sync opt-in move faixas que sairam da playlist para `Removed/`. Nunca apaga.
- Qualidade `Compatible` (MP3, padrao) ou `Best` (maior bitrate estimado pelo `preset`), mantendo a exclusao de
  streams criptografados e previas.
- Retomada por HTTP Range so em progressive, aceitando `206` com `Content-Range` coerente; qualquer outra resposta
  recomeca do zero.
- Download do arquivo original fica fora: o endpoint `/tracks/{id}/download` retorna 401 sem login (regra
  anonima; ver [sem contorno de DRM](sem-contorno-de-drm.md)).

## Alternativas consideradas
- Archive global unico (estilo `--download-archive` do yt-dlp): descartado porque nao permite a mesma faixa em
  duas playlists com numeracao diferente; a biblioteca resolve isso copiando.
- Sync que apaga (scdl `--sync`): descartado por ser destrutivo e irreversivel.
- Hardlinks em vez de copia: descartado por falhar entre volumes e porque cada copia precisa de tags proprias.

## Consequencias
- Mais estado em disco (dois JSON) com escrita atomica; corrupcao cai para "vazio" e so custa rebaixar.
- `DownloadRequest` ganhou `options` e `library_path`; `JobIn` do Tauri aceita os novos campos com padroes seguros.
