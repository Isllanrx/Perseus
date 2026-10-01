# Versão online (Vercel)

A mesma interface do app roda no navegador, no computador, no Android e no iPhone. O áudio nunca passa pelo
servidor: a função da Vercel só resolve metadados e assina as URLs, e o navegador baixa direto do CDN do
SoundCloud. O motivo e as medições estão em [versão web na Vercel](decisions/versao-web-vercel.md).

| | App para Windows | Versão online |
| --- | --- | --- |
| Faixas, álbuns, playlists, perfis e busca | Sim | Sim |
| Capa e metadados (ID3v2.3 / MP4) | Sim | Sim (Ogg Opus sai sem tags) |
| Onde os arquivos vão | Pasta escolhida | Pasta escolhida (Chrome/Edge no computador) ou downloads do navegador; playlists num `.zip` |
| Monitorar playlist, biblioteca local, `Removed/`, limite de banda | Sim | Não (precisam de um processo sempre ligado) |
| Tamanho por download | Sem limite | Até 1.000 faixas (use **Baixar só as primeiras**) |

## Publicar na Vercel

1. Na Vercel, **Add New → Project** e importe o repositório. Deixe **Root Directory** na raiz: o `vercel.json` já
   define instalação (`npm ci --prefix web`), build (`npm --prefix web run build:web`), saída (`web/dist`), a função
   Rust `api/perseus.rs` e os cabeçalhos de segurança.
2. Em **Settings → Build and Deployment**, use Node.js 24.
3. Opcional, em **Settings → Environment Variables**:

   | Variável | Uso |
   | --- | --- |
   | `SOUNDCLOUD_CLIENT_ID` | client_id fixo (32 caracteres); evita a descoberta a cada instância fria |
   | `PERSEUS_LOG` | filtro de log da função (padrão `info`) |

4. Faça o deploy. O primeiro build compila o Rust (cerca de 2 min); os seguintes usam o cache.
5. Em **Settings → Domains**, adicione `perseus.isllan.dev` e crie o `CNAME` `perseus → cname.vercel-dns.com` no
   DNS do `isllan.dev` (se o DNS já estiver na Vercel, o registro é criado sozinho).
6. Em **Firewall → Configure → + New Rule** (o Hobby permite 1 regra de rate limit, gratuita):
   - **If** `Request Path` `Starts with` `/api/`
   - **Then** `Rate Limit` · `Fixed Window` · **Time Window** `60s` · **Request Limit** `150` · chave `IP`
   - Ação `Default (429)` → **Save Rule** → **Review Changes** → **Publish**

## Custo no plano Hobby

O projeto foi ajustado para caber no Hobby, que é gratuito e **não cobra excedente**: se uma cota mensal acabar, a
Vercel pausa o recurso até completar 30 dias, sem fatura. Só existe cobrança se você mudar para o Pro.

| Cota mensal do Hobby | O que o Perseus faz para economizar |
| --- | --- |
| 1.000.000 execuções de função | ~1 por faixa; `config`, `inspect` e `search` ficam no cache do CDN e não executam a função de novo |
| 4 h de CPU ativa | Espera de rede não conta; `SOUNDCLOUD_CLIENT_ID` evita baixar e analisar o site do SoundCloud a cada instância nova |
| 360 GB-h de memória (2 GB fixos, ~180 h) | `maxDuration` de 30 s: nenhuma requisição presa consome mais que isso |
| 10 GB de Fast Origin Transfer | Respostas em gzip; o áudio vai direto do CDN do SoundCloud ao navegador e nunca passa pela Vercel |
| 1.000.000 de requisições no rate limit | Cada visitante fica em ~92 chamadas/min (intervalo aleatório de 400 a 900 ms entre chamadas); a função e o Firewall barram acima de 150/min por IP |

Acompanhe em **Usage** no painel. O Hobby é para uso pessoal e não comercial.

## Rodar localmente

```powershell
cargo run -p perseus-vercel          # função em http://127.0.0.1:3000
npm --prefix web run dev:web         # interface em http://localhost:5173 (proxy de /api)
```
