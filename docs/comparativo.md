# Comparativo

Comparação com as ferramentas mais usadas para baixar do SoundCloud, considerando o que funciona **sem login**.

| Recurso | Perseus | scdl | yt-dlp | SoundCloudExplode | soundcloud-dl (NotTobi) | DownCloud (web) |
| --- | :-: | :-: | :-: | :-: | :-: | :-: |
| Faixas, álbuns e playlists | Sim | Sim | Sim | Sim | Sim | Sim |
| Curtidas, reposts e faixas enviadas | Sim | Sim | Parcial | Parcial | Não | Não |
| Faixas populares e relacionadas | Sim | Não | Parcial | Não | Não | Não |
| Álbuns e playlists do perfil, uma pasta por álbum | Sim | Parcial | Não | Não | Não | Não |
| Busca integrada | Sim | Não | Sim | Sim | Não | Não |
| Aplicativo desktop | Sim | Não | Não | Não | Extensão | Web |
| Ver disponibilidade (DRM, Go+, região) antes de baixar | Sim | Não | Não | Não | Não | Não |
| Downloads paralelos | Sim | Não | Não | Não | Não | Não |
| Escolha de qualidade pelo bitrate real | Sim | Parcial | Sim | Parcial | Não | Não |
| Não baixar de novo o que já existe | Sim | Sim | Sim | Não | Não | Não |
| Monitorar playlist e baixar faixas novas | Sim | Não | Não | Não | Não | Não |
| Retomada por HTTP Range | Sim | Não | Sim | Não | Não | Não |
| Validação de integridade antes de gravar | Sim | Não | Parcial | Não | Não | Não |
| Template de nome e filtros de duração | Sim | Sim | Sim | Não | Não | Não |
| Playlist `.m3u8` | Sim | Parcial | Não | Não | Não | Não |
| Reaproveitar faixas de outras pastas sem rede | Sim | Não | Não | Não | Não | Não |
| Sincronizar remoções sem apagar arquivos | Sim | Não | Não | Não | Não | Não |
| Verificação de espaço em disco | Sim | Não | Não | Não | Não | Não |
| Limite de banda | Sim | Não | Sim | Não | Não | Não |
| Interface em 11 idiomas (inclui RTL) | Sim | Não | Não | Não | Não | Não |

O arquivo original enviado pelo artista (WAV, FLAC) só é liberado para contas logadas e, por isso, fica fora do
Perseus, que funciona apenas de forma anônima.

## Desempenho em relação à v0.2

Medições reais (Windows 11, mesma conexão), playlist de 11 faixas / 50 MB:

| | Perseus 0.2 (Python) | Perseus 1.0 (Rust) |
| --- | --- | --- |
| Instalação | zip de 52,5 MB (runtime Python embutido) | instalador de 5,2 MB |
| Início de um download | subprocesso Scrapy relança o executável inteiro | tarefa assíncrona no mesmo processo |
| HLS | segmentos em série; uma falha reinicia a faixa | segmentos em paralelo, retry por segmento |
| Playlist de 11 faixas | — | 2,0 s pela CLI com 8 downloads simultâneos (25 MB/s); 3,4 s pelo aplicativo com 4 |

## Projetos estudados

O Perseus foi construído estudando estes projetos. Nenhum código foi copiado.

| Projeto | O que o Perseus aprendeu com ele |
| --- | --- |
| [scdl](https://github.com/scdl-org/scdl) — scdl-org | Recursos que usuários esperam: perfis, likes, arquivo de histórico, sincronização |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Referência de extração do SoundCloud usada por boa parte do ecossistema |
| [downcloud](https://github.com/yaaaarn/downcloud) — yaaaarn | Foco em velocidade e medição comparativa |
| [scdl](https://github.com/imthaghost/scdl) — imthaghost | Hidratação em lote da playlist, segmentos HLS e recusa explícita de DRM |
| [SoundCloudExplode](https://github.com/jerry08/SoundCloudExplode) — jerry08 | Organização de uma API cliente para faixas, playlists e busca |
| [soundcloud-dl](https://github.com/NotTobi/soundcloud-dl) — NotTobi | Normalização de nomes e opções de qualidade vistas pelo usuário |
