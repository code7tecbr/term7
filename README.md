# term7

Terminal com interface moderna, feito com **Tauri 2 + Vue 3 + TypeScript + xterm.js**.
O backend em Rust abre o shell via [`portable-pty`](https://crates.io/crates/portable-pty)
e manda a saída em bytes brutos pro frontend, que renderiza com WebGL.

## Pré-requisitos (Linux)

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
- [ ] Integração com o Claude CLI (aviso quando termina, Shift+Enter, comentar no diff)
- [ ] Blocos de comando (comando + saída agrupados)
- [ ] Paleta de comandos (Ctrl+Shift+P)
- [ ] Temas e config
- [ ] v2: migrar pra renderização nativa em Rust (GPU)
