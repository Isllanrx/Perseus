# Arquitetura em fatias verticais

**Situacao:** Em vigor (2026-09-29)

## Contexto
A v0.1 ja tinha pastas por "feature", mas a apresentacao era horizontal e a logica vazava: selecao de
transcoding duplicada entre spider e resolver, headers HTTP repetidos em tres modulos, watcher imprimindo com
Rich direto (a GUI nao via nada). Mudancas simples tocavam muitos arquivos.

## Decisao
- `features/<caso de uso>` contem o caso de uso inteiro: `inspect`, `download` (planejamento, engines, audio,
  tagging, crawler), `watch` (compoe `download` apenas pela API publica do pacote).
- `shared/` e o kernel transversal sem regra de fatia: config, erros, http, retry, log, filesystem e a
  integracao `shared/soundcloud/` (auth, client, modelos, transcoding, urls), usada por todas as fatias.
- `presentation/` (CLI e web) so traduz entrada/saida; nenhuma regra de negocio.

## Alternativas consideradas
- **Camadas (domain/application/infrastructure)**: mais cerimonia para um app pequeno; uma feature nova
  tocaria 3-4 camadas.
- **Manter a estrutura v0.1**: nao resolvia a duplicacao nem o acoplamento da UI ao Rich.

## Consequencias
- Nova fonte (ex.: perfis de usuario) vira uma fatia nova reaproveitando `download`.
- Risco: `shared/soundcloud` crescer como "deus"; manter nele apenas integracao com a API, sem politica de fatia.
