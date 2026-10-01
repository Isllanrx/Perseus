# Ignorar robots.txt

**Situacao:** Em vigor (2026-09-29)

## Contexto
O Perseus nao consulta robots.txt. Na v0.2, o Scrapy era usado apenas contra a API JSON
`api-v2.soundcloud.com` (resolve e lotes de faixas), consumida da mesma forma que o player web publico; nao ha
navegacao/crawling de paginas HTML. As chamadas via `requests` nunca consultam robots.txt.

## Decisao
`ROBOTSTXT_OBEY = False` fixo em `crawler/settings.py`.

## Alternativas consideradas
- **Obedecer robots.txt**: bloquearia os endpoints da API usados pelo proprio player e inviabilizaria o projeto.

## Consequencias
- Responsabilidade de uso e do usuario (README/Disclaimer).
- Mitigacoes de impacto no servico: AutoThrottle, `DOWNLOAD_DELAY` com jitter, concorrencia limitada por
  dominio, retry respeitando `Retry-After`, lotes de 50 IDs.

## Atualizacao (2026-09-30, reescrita em Rust)
Sem Scrapy, a decisao continua valendo por construcao: o cliente Rust (reqwest) nunca consulta robots.txt e so
acessa a API JSON e a CDN de midia. Mitigacoes de impacto no v1.0: concorrencia limitada (`workers` <= 16 faixas,
6 segmentos HLS por faixa, 4 lotes de faixas por vez), retry com backoff exponencial e jitter respeitando
`Retry-After` (teto de 60 s) e lotes de 50 IDs. O AutoThrottle do Scrapy deixou de existir.
