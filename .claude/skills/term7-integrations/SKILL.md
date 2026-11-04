---
name: term7-integrations
description: >
  Orientation for changing term7's GitHub (gh CLI: PR, checks, action runs) and Claude Code integrations
  (hooks/TERM7_PANE tab state, suggested commands, subagents menu, MCP list, usage limits via statusLine
  and headless get_usage, CLAUDE_CONFIG_DIR). These are the core of the product and must not regress. Use
  whenever you touch gh.rs, claude.rs, suggest.rs, agents.rs, usage.rs, github.ts or useGithub.ts, parse
  anything from ~/.claude (sessions, projects/*.jsonl transcripts, subagents, settings.json), or change
  polling — even for a small parser tweak. Gatilhos em português: "integração com o GitHub/Claude",
  "status do PR", "checks/actions", "hooks do Claude", "agentes", "comandos sugeridos", "uso do Claude",
  "MCP", "parser do jsonl".
---

# Integrações com o GitHub e o Claude Code

Estas integrações são o coração do term7 e hoje funcionam bem pra quem mantém o projeto. O objetivo de
qualquer mudança aqui é **não quebrar o que já funciona**: leia o arquivo inteiro antes de mexer, mude o
mínimo e prove com teste.

## Mapa

| Arquivo | O que faz |
|---|---|
| `src-tauri/src/gh.rs` | `gh_pr`, `gh_runs`, `open_url`. Roda o `gh` na pasta da aba; devolve o JSON cru (`GhResult`: `missing`, `unauthenticated`, `none`, `ok`, `error`) e quem interpreta é o frontend. |
| `src/lib/github.ts` | Tipos e interpretação do JSON do `gh` (PR, checks, runs). |
| `src/lib/useGithub.ts` | Polling: `IDLE_MS = 60_000`, `PENDING_MS = 15_000` (com check/action pendente). |
| `src-tauri/src/claude.rs` | Hooks do Claude (estado da aba), leitura/gravação do `settings.json` do Claude, `claude_proc` (binário, cwd e ambiente via `/proc`) e a lista de MCPs. |
| `src-tauri/src/suggest.rs` | Comandos sugeridos: acha a sessão do processo e extrai comandos das últimas respostas. |
| `src-tauri/src/agents.rs` | Subagentes da sessão e a linha do tempo de cada um. |
| `src-tauri/src/usage.rs` | Uso da assinatura (limites de 5 h, 7 dias e por modelo) e contexto da sessão. |

## Fontes de dados

**GitHub.** Só pelo binário `gh`, que cuida do login e de achar o repositório pelos remotes.
- Compatível com o **gh 2.4** (Ubuntu 22.04): use só os campos de `PR_FIELDS`/`RUN_FIELDS` de `gh.rs`;
  sem `run list --branch` nem `workflowName`. Campo novo = confirme que existe no 2.4.
- Não encurte os intervalos de 60 s / 15 s: são pra não gastar o limite da API do usuário.

**Claude Code.** Tudo é lido do processo `claude` em primeiro plano na aba e dos arquivos dele.
- **Pasta de configuração:** `CLAUDE_CONFIG_DIR` lido do **ambiente do processo** (`/proc/<pid>/environ`),
  não do app: a variável pode estar só no `~/.zshrc`, que o app aberto pelo menu não vê. Sem ela,
  `~/.claude`. Nunca assuma `~/.claude` fixo.
- **Estado da aba (hooks):** o term7 exporta `TERM7_PANE` no shell de cada aba; os hooks que ele instala
  no `settings.json` do Claude gravam o estado em `$XDG_RUNTIME_DIR/term7/claude-<pane>`. Fora do term7
  a variável não existe e o hook não faz nada. A instalação só mexe nas entradas marcadas com
  `TERM7_PANE`, preserva o resto do arquivo, faz cópia do original na primeira vez e troca atômica.
- **Sessão e histórico:** `<config>/sessions/<pid>.json` dá o id da sessão e o `cwd`;
  `<config>/projects/<slug>/<id>.jsonl` é o histórico (slug: `/home/x/a.b` → `-home-x-a-b`). Como o `cwd`
  acompanha o `cd` dentro do Claude, há fallback procurando o `<id>.jsonl` em todas as pastas de
  `projects/`. Só o fim do arquivo é lido (ele cresce a sessão toda) e há cache por arquivo.
- **Subagentes:** `projects/<slug>/<sessão>/subagents/agent-<id>.meta.json` (tipo, descrição, modelo, se
  terminou) e `agent-<id>.jsonl` (mensagens e uso de tokens).
- **MCPs:** nomes do `.mcp.json` e do escopo local no `.claude.json` (da pasta até antes da home), ou os
  globais a pedido; o status vem de `claude mcp get <nome>` com o binário e o ambiente da aba. Só Status,
  URL e Command são lidos, e o endereço é limpo de credenciais: headers e env (onde ficam tokens) nunca.
- **Uso:** os `rate_limits` só chegam ao comando do `statusLine`. O term7 põe um wrapper (marcado com
  `TERM7_USAGE=1`) na frente do comando do usuário, guarda o JSON em
  `$XDG_RUNTIME_DIR/term7/usage-<pasta>.json` e repassa ao original, que desenha a linha igual; o original
  fica em `<config>/term7-statusline.json`, pra desfazer. Os limites por modelo vêm de um Claude headless
  igual ao da aba com o control request `get_usage`, a cada 5 min, sem hooks e sem mensagem (não gasta uso).

## Regras ao mudar

- **Parser mudou, teste junto.** Toda mudança em leitura de `.jsonl`, `meta.json`, `settings.json`,
  saída do `gh` ou do `claude mcp get` vem com teste unitário no `#[cfg(test)] mod tests` do arquivo,
  usando fixture com o **formato real** (copie uma linha de verdade e anonimize), não um JSON inventado
  mínimo. Mantenha os testes existentes passando; se um precisa mudar, explique por quê no PR.
- Formato desconhecido ou campo ausente degrada em silêncio (lista vazia, `None`), nunca derruba a aba.
- Não altere nada do usuário que não seja do term7 no `settings.json` do Claude, e mantenha o desfazer.
- Nunca leia nem mande pro frontend tokens, headers ou variáveis de ambiente com credenciais.
- I/O e processos (`gh`, `claude`, `/proc`, arquivos) em comando `async` + `spawn_blocking`.
- Valide como na skill `term7-dev` (inclui `cargo test` no Docker) e teste no app instalado, com um
  Claude rodando numa aba e um repositório com PR, antes de abrir o PR.
