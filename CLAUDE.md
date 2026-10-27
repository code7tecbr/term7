# term7

Terminal desktop com interface moderna. **Tauri 2** (backend Rust) + **Vue 3 + TypeScript** (frontend) + **xterm.js** (renderização WebGL).
A v1 é web-based de propósito; a v2 planeja migrar pra renderização nativa em Rust (GPU).

## Comandos

```sh
npm run build                     # type-check (vue-tsc) + build do frontend; rode depois de mexer em src/
docker compose run --rm build     # compila o .deb em dist-deb/ (mesmo Ubuntu 22.04 do CI)
scripts/install-local.sh          # instala o .deb em ~/.local (sem sudo) pra testar de verdade
docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo check"   # checa só o Rust
```

- A máquina de desenvolvimento **não tem Rust instalado**: todo `cargo` roda dentro do container.
- **Não teste interação pelo Docker** (`compose run dev`): lá não há GPU e tudo fica lento. Compile no container, instale com `scripts/install-local.sh` e abra `~/.local/bin/term7`.
- Não há testes automatizados ainda. Validação = `npm run build` + `cargo check` sem warnings + testar no app.

## Estrutura

```
src/
  App.vue                 layout: sidebar | terminais (+ terminal inferior) | visualizador | diff
  components/             TabSidebar, TerminalView, DiffPanel, FileViewer (lazy), BranchPicker, StatusBar, ContextMenu
  state/tabs.ts           abas e grupos (store reativo); regra: abas de um grupo são sempre contíguas
  state/session.ts        salva/restaura o layout
  state/viewer.ts         arquivos abertos no visualizador
  lib/pty.ts              ponte com o backend (invoke + Channel de bytes brutos)
  lib/git.ts              tipos e chamadas de git; parseDiff
  lib/shortcuts.ts        atalhos globais (o terminal ignora essas teclas)
src-tauri/src/
  pty.rs                  sessões de shell (spawn/write/resize/kill/info via /proc)
  git.rs                  status, diff, branches, checkout, fetch (usa o binário `git`)
  files.rs                leitura de arquivo pro visualizador
  session.rs              session.json em ~/.local/share/br.tec.code7.term7/
```

## Convenções

- **Idioma:** comentários, textos da interface e docs em português do Brasil, com acentuação. Identificadores em inglês.
- **Comentários** explicam o *porquê*, curtos. Nada de comentário que repete o código.
- **Comandos Tauri** que fazem I/O ou chamam processos (git, arquivos) são `async` + `spawn_blocking`: comando síncrono roda na thread da janela e trava a interface.
- **Saída do PTY** vai como bytes brutos (`InvokeResponseBody::Raw`), nunca JSON.
- **Git em polling** usa `GIT_OPTIONAL_LOCKS=0` pra não disputar o `index.lock` com o usuário.
- **Botões** que não devem roubar o foco do terminal usam `@mousedown.prevent`.
- Novos comandos Tauri: registrar em `src-tauri/src/lib.rs`; permissões de janela em `src-tauri/capabilities/default.json`.
- **Dependências:** TypeScript fica em `~5.9` (o `vue-tsc` não suporta o 7).
- O Wayland identifica a janela pelo `identifier` (`br.tec.code7.term7`); o `.desktop` em `src-tauri/term7.desktop` precisa manter `StartupWMClass` igual a ele.

## Git e releases

- **Conventional Commits** (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`): o semantic-release calcula a versão a partir deles.
- Nunca commitar direto na `main`: todo push na `main` gera release, compila o `.deb` e apaga as releases além das 3 mais recentes.
- Commit e push só quando o usuário pedir.
