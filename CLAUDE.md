# term7

Terminal desktop com interface moderna. **Tauri 2** (backend Rust) + **Vue 3 + TypeScript** (frontend) + **xterm.js** (renderização WebGL).
A v1 é web-based de propósito; a v2 planeja migrar pra renderização nativa em Rust (GPU).

## Comandos

```sh
npm test                          # testes do frontend (Vitest)
npm run build                     # type-check (vue-tsc) + build do frontend; rode depois de mexer em src/
docker compose run --rm build     # compila o .deb em dist-deb/ (mesmo Ubuntu 24.04 do CI)
scripts/install-local.sh          # instala o .deb em ~/.local (sem sudo) pra testar de verdade
docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo check && cargo test"   # checa e testa o Rust
```

- A máquina de desenvolvimento **não tem Rust instalado**: todo `cargo` roda dentro do container.
- **Não teste interação pelo Docker** (`compose run dev`): lá não há GPU e tudo fica lento. Compile no container, instale com `scripts/install-local.sh` e abra `~/.local/bin/term7`.
- Validação = `npm test` (Vitest, `src/**/*.test.ts`) + `npm run build` + `cargo check`/`cargo test` sem warnings + testar no app. O CI roda os testes em todo PR.
- Lógica ou parser novo vem com teste: no Rust, no `mod tests` do arquivo, com dados no formato real (anonimizados).

## Estrutura

```
src/
  App.vue                 layout: sidebar | área principal (abas: terminal + arquivos; terminal inferior) | explorador
  components/             TabSidebar, TerminalView, EditorTabs, FileView (lazy), PreviewPopup, FileExplorer, BranchPicker, TopBar (Claude: agentes e MCP; microfone, anexo, configurações), AgentsMenu, SettingsPanel, McpMenu, StatusBar (pasta, git, PR), ContextMenu
  state/tabs.ts           abas e grupos (store reativo); regra: abas de um grupo são sempre contíguas
  state/session.ts        salva/restaura o layout
  state/editor.ts         arquivos abertos na área principal (cada aba lateral tem os seus)
  state/anon.ts           modo anônimo (gravar tela): troca a fonte de tudo pela embaralhada de src/assets (gerada por scripts/anon-font.py)
  state/review.ts         comentários de revisão em trechos de arquivo; vão pro Claude da aba como colagem (sem Enter)
  lib/pty.ts              ponte com o backend (invoke + Channel de bytes brutos)
  lib/git.ts              tipos e chamadas de git
  lib/github.ts           PR, checks e runs (via gh); useGithub.ts faz o polling
  lib/links.ts, paths.ts  URLs e caminhos de arquivo clicáveis na saída do terminal
  lib/shortcuts.ts        atalhos globais (o terminal ignora essas teclas)
  lib/suggestions.ts      botões ▶/⤓ ao lado dos comandos que o Claude sugeriu (procura o texto na tela)
  lib/settings.ts         configurações, ditado (voice_*) e seletor de arquivos; shellQuote dos caminhos colados
  lib/update.ts           confere/instala a versão nova (botão ao lado da versão, na barra)
  lib/api.ts, screens.ts  API das extensões: atende os pedidos do api.rs (abas, tela, mandar pro Claude com fila)
  state/extStatus.ts      status publicado por extensões (status.set); ExtStatus.vue desenha na barra de status
src-tauri/src/
  pty.rs                  sessões de shell (spawn/write/resize/kill/info via /proc)
  git.rs                  status, ls-files, branches, checkout, fetch, info do repo (usa o binário `git`)
  gh.rs                   PR e runs via `gh`; open_url (xdg-open, só http(s))
  claude.rs               Claude Code: avisos via hooks (TERM7_PANE) e MCPs do projeto da aba, ou os globais a pedido (`claude mcp get` com o binário/ambiente do processo)
  suggest.rs              comandos sugeridos: lê o histórico da sessão do Claude (sessions/<pid>.json -> projects/<pasta>/<id>.jsonl)
  agents.rs               subagentes do Claude da aba: lê <sessão>/subagents/agent-*.meta.json e .jsonl
  clipboard.rs            cópia pedida pelo terminal (OSC 52) via GTK: o WebKit recusa writeText fora de clique/tecla
  files.rs                leitura de arquivo pro visualizador; resolve_paths (caminhos clicáveis)
  settings.rs             settings.json (600); a chave da API de transcrição nunca volta pro frontend
  voice.rs                ditado: pw-record/arecord -> API compatível com a OpenAI (curl, chave pelo stdin) ou whisper.cpp
  session.rs              session.json em ~/.local/share/br.tec.code7.term7/
  update.rs               atualização: última release via `gh api`, baixa o .deb e instala com `pkexec apt-get`
  api.rs                  API local das extensões: socket $XDG_RUNTIME_DIR/term7/api.sock (desligada por padrão), repassa ao frontend
  definition.rs           ir pra definição (Ctrl+clique): git grep + linhas com cara de definição; imports relativos
extensions/               extensões fora do app, via API (mcp/: servidor MCP; statusbot/: status na barra), em Python; docs em extensions/README*.md
```

## Convenções

- **Idioma:** comentários, textos da interface e docs em português do Brasil, com acentuação. Identificadores em inglês.
- **Comentários** explicam o *porquê*, curtos. Nada de comentário que repete o código.
- **Comandos Tauri** que fazem I/O ou chamam processos (git, arquivos) são `async` + `spawn_blocking`: comando síncrono roda na thread da janela e trava a interface.
- **Saída do PTY** vai como bytes brutos (`InvokeResponseBody::Raw`), nunca JSON.
- **`gh`** precisa funcionar com a versão 2.4 (Ubuntu 22.04): sem `run list --branch` nem `workflowName`. Consultas ao GitHub são espaçadas (60 s, 15 s com action rodando).
- **Git em polling** usa `GIT_OPTIONAL_LOCKS=0` pra não disputar o `index.lock` com o usuário.
- **Botões** que não devem roubar o foco do terminal usam `@mousedown.prevent`.
- Novos comandos Tauri: registrar em `src-tauri/src/lib.rs`; permissões de janela em `src-tauri/capabilities/default.json`.
- **Dependências:** TypeScript fica em `~5.9` (o `vue-tsc` não suporta o 7). As licenças das dependências vão no `.deb` (`scripts/third-party.mjs`, roda no build).
- **Docs públicas** (README, CONTRIBUTING) têm versão em inglês e em português (`*.pt-BR.md`): mude as duas juntas.
- O Wayland identifica a janela pelo `identifier` (`br.tec.code7.term7`); o `.desktop` em `src-tauri/term7.desktop` precisa manter `StartupWMClass` igual a ele.

## Git e releases

- **Conventional Commits** (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`): o semantic-release calcula a versão a partir deles.
- **Branches:** PRs de feature/correção partem da `dev` e voltam pra `dev`. A `main` só recebe o PR `dev → main`, e cada merge nela gera release (compila o `.deb` e apaga as releases além das 3 mais recentes). Em PR o workflow só compila o `.deb`.
- Nunca commitar direto na `dev` nem na `main`, e nunca apagar a `dev` (nem com `--delete-branch` no merge `dev → main`).
- Commit e push só quando o usuário pedir.
