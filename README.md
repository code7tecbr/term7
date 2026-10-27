# term7

Terminal com interface moderna, feito com **Tauri 2 + Vue 3 + TypeScript + xterm.js**.
O backend em Rust abre o shell via [`portable-pty`](https://crates.io/crates/portable-pty)
e manda a saída em bytes brutos pro frontend, que renderiza com WebGL.

## Pré-requisitos (Linux)

Pra usar, o term7 precisa do `git` e do [`gh`](https://cli.github.com/) (GitHub CLI): o `git`
alimenta o painel de alterações e as branches; o `gh` mostra o PR da branch e o estado das actions.
O `.deb` já declara os dois como dependência, então o `apt` instala junto. Depois, uma vez só:

```sh
gh auth login
```

O `gh` dos repositórios do Ubuntu 22.04 (2.4) funciona; o do [repositório oficial](https://github.com/cli/cli/blob/trunk/docs/install_linux.md) também.

Pra desenvolver:

```sh
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Bibliotecas do Tauri (Ubuntu/Pop!_OS/Debian)
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

## Rodando

```sh
npm install
npm run tauri dev      # abre o app com hot reload
npm run tauri build    # gera o .deb em src-tauri/target/release/bundle/deb/
```

## Com Docker (sem instalar Rust na máquina)

```sh
docker compose run --rm build   # gera o .deb em ./dist-deb/ (mesmo ambiente do CI)
docker compose run --rm dev     # abre o app na sua tela com hot reload

# usando o seu zsh/oh-my-zsh e fontes dentro do container
docker compose -f compose.yaml -f compose.zsh.yaml run --rm dev
```

> Dentro do container a GPU geralmente não é usada, então a interface fica mais lenta.
> Pra sentir a velocidade real, instale o `.deb` gerado: `sudo apt install ./dist-deb/term7_*.deb`,
> ou só pro seu usuário, sem sudo: `scripts/install-local.sh` (`--remove` desinstala).

## Estrutura

```
src/                  frontend Vue
  components/         TerminalView (xterm.js)
  lib/pty.ts          ponte com o backend (invoke + Channel)
  lib/theme.ts        tema padrão
src-tauri/src/pty.rs  sessões de shell (spawn, write, resize, kill)
```

## Versionamento e releases

A versão é calculada pelo [semantic-release](https://semantic-release.gitbook.io) a partir das
mensagens de commit ([Conventional Commits](https://www.conventionalcommits.org/pt-br/)):

| Commit                         | Versão  |
|--------------------------------|---------|
| `fix: ...`                     | patch   |
| `feat: ...`                    | minor   |
| `feat!: ...` / `BREAKING CHANGE` | major |
| `chore:`, `docs:`, `refactor:` | sem release |

A cada push na `main`, o workflow `.github/workflows/release.yml` cria a release, compila o
`.deb`, anexa na release e apaga as releases mais antigas, deixando só a atual + 2 anteriores.

## Roadmap

- [x] Abas na lateral, com nome e cor por aba
- [x] Grupos de abas (nome, cor, recolher) e arrastar/soltar
- [x] Sessão restaurada ao reabrir (abas, grupos, pastas)
- [x] Terminal inferior por aba (`Ctrl+'`)
- [x] Painel de diff do git (`Ctrl+Shift+G`) e visualizador de arquivo
- [x] Troca de branch pela barra de status
- [x] PR da branch e estado das actions na barra de status (indica worktree)

O que falta está nas [issues](https://github.com/code7tecbr/term7/issues):

- **Claude CLI** ([label `claude-cli`](https://github.com/code7tecbr/term7/issues?q=is%3Aissue+label%3Aclaude-cli)):
  aviso ao terminar ([#8](https://github.com/code7tecbr/term7/issues/8)), Shift+Enter ([#9](https://github.com/code7tecbr/term7/issues/9)), comentar no diff ([#10](https://github.com/code7tecbr/term7/issues/10)), sessão em worktree ([#11](https://github.com/code7tecbr/term7/issues/11))
- **Terminal:** caminhos clicáveis ([#1](https://github.com/code7tecbr/term7/issues/1)), blocos de comando ([#2](https://github.com/code7tecbr/term7/issues/2)), paleta de comandos ([#3](https://github.com/code7tecbr/term7/issues/3))
- **Visual:** temas e config ([#4](https://github.com/code7tecbr/term7/issues/4)), ícone definitivo ([#5](https://github.com/code7tecbr/term7/issues/5))
- **Visualizador:** edição de arquivo ([#6](https://github.com/code7tecbr/term7/issues/6))
- **Plataformas:** macOS e Windows ([#7](https://github.com/code7tecbr/term7/issues/7))
- **v2:** renderização nativa em Rust ([#12](https://github.com/code7tecbr/term7/issues/12))
