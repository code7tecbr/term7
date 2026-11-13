---
name: term7-dev
description: >
  How to build, test and validate a change in term7 (Tauri 2 + Vue 3/TypeScript + xterm.js), plus the code
  conventions to follow when editing src/ or src-tauri/. Use whenever you change code in this repo, need to
  compile, run tests, type-check, run cargo check/test/clippy, build the .deb, install it locally or try the
  app — even if the user only says "fix this" or "add X". Gatilhos em português: "compila", "roda os testes",
  "valida a mudança", "gera o .deb", "instala pra testar", "cargo check", "testar no app", "mexer no Rust/Vue".
---

# Desenvolvendo no term7

## Validar uma mudança

Rode tudo isto antes de dar a mudança por pronta. O CI compila o `.deb` no PR, mas warnings e testes são
responsabilidade de quem muda.

```sh
npm run build        # vue-tsc --noEmit + vite build; obrigatório depois de mexer em src/
npm test             # testes do frontend (vitest)

# Rust: sempre pelo container (o ambiente de referência não tem Rust instalado)
docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo check"
docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo test"
# clippy: o rustup do container é o perfil minimal, sem o clippy
docker compose run --rm --no-deps dev bash -c "cd src-tauri && rustup component add clippy && cargo clippy"
```

- `cargo check` precisa passar **sem warnings**, não só sem erros.
- Os testes Rust ficam no próprio arquivo, em `#[cfg(test)] mod tests`, com nomes em português
  (ex.: `fn pasta_do_projeto()`). Siga o mesmo padrão.
- Se `npm test` ainda não existir no `package.json`, avise em vez de inventar outro comando.

## Testar o app de verdade

Interação (digitar, clicar, desempenho) se testa no app nativo, não no Docker. Use a build de
teste, que roda ao lado da release instalada sem mexer nela:

```sh
docker compose run --rm build-test   # .deb "term7-dev" em dist-test/ (outro nome e identificador)
scripts/run-test.sh                  # extrai em dist-test/root e abre, sem instalar nada
```

A build de teste tem configurações, sessão e janela próprias (`br.tec.code7.term7.dev`). **Não use
`scripts/install-local.sh` pra testar**: ele grava `~/.local/bin/term7` e o atalho do lançador, que passam
por cima da release que o mantenedor usa no dia a dia.

`docker compose run --rm dev` abre a interface, mas o container não tem GPU: tudo fica lento e o
xterm.js com WebGL não se comporta como no app real. Use o Docker só pra compilar.
Quem tem Rust e as libs do Tauri na máquina pode usar `npm run tauri dev` (hot reload).

## Convenções que importam ao editar

- **Comandos Tauri que fazem I/O ou chamam processos** (git, gh, arquivos, `/proc`) são `async` e usam
  `spawn_blocking`: comando síncrono roda na thread da janela e trava a interface.
- **Comando Tauri novo:** registre em `src-tauri/src/lib.rs`; permissões de janela em
  `src-tauri/capabilities/default.json`.
- **Saída do PTY** vai como bytes brutos (`InvokeResponseBody::Raw`), nunca JSON.
- **`gh`** precisa funcionar com a versão 2.4 (Ubuntu 22.04): sem `run list --branch` nem `workflowName`.
  Consultas ao GitHub são espaçadas (60 s; 15 s com action rodando).
- **Git em polling** usa `GIT_OPTIONAL_LOCKS=0`, pra não disputar o `index.lock` com o usuário.
- **Botões que não devem roubar o foco do terminal** usam `@mousedown.prevent`.
- **TypeScript fica em `~5.9`**: o `vue-tsc` não suporta o 7. Não suba essa dependência.
- O `identifier` (`br.tec.code7.term7`) e o `StartupWMClass` de `src-tauri/term7.desktop` precisam
  continuar iguais (é por ele que o Wayland identifica a janela).
- **Idioma:** comentários, textos da interface e docs em português do Brasil, com acentuação;
  identificadores em inglês. Comentários curtos que explicam o *porquê*, nunca repetem o código.

A estrutura de pastas e o papel de cada arquivo estão no `CLAUDE.md` da raiz. Se a mudança mexe com
GitHub ou Claude Code, veja também a skill `term7-integrations`.
