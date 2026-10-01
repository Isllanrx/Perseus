# Sem contorno de DRM nem previas disfarcadas

**Situacao:** Em vigor (2026-09-29)

## Contexto
O objetivo e baixar "qualquer" musica publica sem conta, e o projeto sera publicado no GitHub. O fallback da
v0.1 (`transcodings[0]`) podia escolher `ctr/cbc-encrypted-hls` (DRM de faixas de gravadora) e gerar lixo, ou
baixar previas de 30 s do Go+ como se fossem a faixa completa.

## Decisao
- `choose_transcoding` so aceita `progressive` e `hls` abertos, nunca `snipped`.
- O fetcher HLS recusa manifestos com `#EXT-X-KEY` diferente de `NONE`.
- Faixas sem stream aberto aparecem como **indisponiveis com motivo** (DRM, previa Go+, bloqueio regional).

## Alternativas consideradas
- **Descriptografar streams**: violaria leis anti-circumvention (DMCA 1201 e equivalentes) e os ToS; inviavel
  para um projeto publico.
- **Baixar previas marcando no nome**: engana o usuario e polui a biblioteca.

## Consequencias
- Cobertura menor que ferramentas que usam OAuth Go+, por escolha.
- Mensagens claras reduzem issues do tipo "faixa X nao baixa".
