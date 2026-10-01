# Política de segurança

## Versões com suporte

| Versão | Suporte |
| --- | --- |
| Última release publicada | Sim |
| Versões anteriores | Não. Atualize para a última release |

## Como relatar uma vulnerabilidade

Não abra issues públicas para falhas de segurança. Relate de forma privada pelo
[GitHub Security Advisories](https://github.com/Isllanrx/Perseus/security/advisories/new).

Inclua, se possível:

- a versão do Perseus e o sistema operacional;
- os passos para reproduzir;
- o impacto esperado;
- o log da execução (pasta `%LOCALAPPDATA%\Perseus\data\logs` ou `--log-format json --log-file perseus.log` na CLI),
  sem dados pessoais.

Você recebe uma confirmação em até 7 dias. Correções de problemas confirmados são publicadas numa nova release, e o
advisory é divulgado depois que a correção estiver disponível.

## Escopo

Estão no escopo, por exemplo:

- execução de comandos IPC do aplicativo a partir de conteúdo que não seja a própria interface (CSP, capabilities
  do Tauri);
- execução de código, leitura ou escrita de arquivos fora da pasta de destino (path traversal em nomes de arquivo);
- download de bytes de hosts fora da allowlist de mídia do SoundCloud;
- consumo de recursos sem limite (tamanho de arquivo, manifesto HLS, respostas da API);
- vazamento de dados em logs;
- problemas no instalador distribuído ou no processo de release (checksums, proveniência).

Fora do escopo:

- falhas do próprio SoundCloud ou mudanças na API pública dele;
- faixas indisponíveis por DRM, SoundCloud Go+ ou bloqueio regional. O Perseus não contorna proteções por design;
- avisos do SmartScreen ou do Defender sobre o instalador sem assinatura de código, quando o SHA-256 confere com o
  `SHA256SUMS` da release.
