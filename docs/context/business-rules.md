# Regras de negocio

- Somente conteudo reproduzivel anonimamente. Sem login, sem OAuth, sem DRM.
- Preferencia de stream (qualidade Compativel, padrao): progressive MP3 > progressive outros > HLS MP3 > HLS AAC > HLS Opus. Qualidade Melhor: maior bitrate estimado (`preset`), empate pela mesma ordem.
- Arquivo original (`/download`) exige login: fora do produto.
- Numeracao = posicao na playlist completa, mesmo quando so faixas novas sao baixadas (watch).
- Pasta: `<artista da playlist> - <titulo>`; faixas avulsas em `Single Tracks/`.
- Arquivo existente e valido (mesmo com outro prefixo numerico) e reaproveitado, nunca rebaixado.
- `/albums` e `/sets` do perfil: uma pasta por playlist. Abas (likes, tracks, reposts, popular-tracks) e relacionadas viram uma playlist virtual.
- Archive por pasta (`.perseus-archive.json`) manda sobre o nome do arquivo; biblioteca global copia faixa ja baixada em outra pasta e reescreve as tags para o novo contexto.
- Sync (opt-in) move para `Removed/` o que saiu da playlist; nunca apaga. Desligado com `limit` ou download parcial (`only_track_ids`), porque a lista vista seria incompleta.
- Nome de arquivo e unico por pasta: colisao (template sem `{id}` ou nome truncado) recebe ` [id]`; nunca se
  reaproveita arquivo de outra faixa.
- Dois downloads da mesma faixa na mesma pasta ao mesmo tempo (no app) sao serializados; o segundo reaproveita.
- Filtro de duracao marca a faixa como indisponivel com codigo `filtered`; template de nome precisa de `{title}` ou `{id}`.
- Espaco em disco estimado (bitrate x duracao) e checado antes de baixar; falta de espaco e erro fatal `insufficient_space`.
- Watch: faixa concluida ou indisponivel e "settled"; falhas sao retentadas em ciclos ate `WATCH_MAX_TRACK_ATTEMPTS`, depois abandonadas com log de erro.
- Erro de dominio (404, recurso nao suportado, URL invalida) encerra a execucao sem retry; falhas transitorias (rede, 429, 5xx, arquivo truncado) sao retentadas com backoff.
- Relatorio `ok` exige: sem erro fatal, sem falhas, sem cancelamento. Indisponiveis nao quebram `ok`.
