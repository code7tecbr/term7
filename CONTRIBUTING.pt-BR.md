# Contribuindo com o term7

[English](CONTRIBUTING.md) · **Português**

Valeu pelo interesse! Este guia mostra como relatar um problema, montar o ambiente, testar e mandar sua mudança.

## Issues

1. [Procure nas issues](https://github.com/code7tecbr/term7/issues?q=is%3Aissue) antes: talvez já exista.
2. Abra pelo [modelo certo](https://github.com/code7tecbr/term7/issues/new/choose): **bug** ou **ideia**. Issue
   em branco fica desligada, pra todas virem com as informações que precisamos.
3. Pra bug, diga a versão do term7 (canto direito da barra de status), a distro e o desktop, e, se for nas
   integrações, a versão do `gh` e do `claude`. Print ou GIF ajuda muito.
4. Falha de segurança **não** vai em issue: veja o [SECURITY.md](SECURITY.md).

Escreva em português ou inglês, como preferir.

### O que o mantenedor usa todo dia

O term7 é usado diariamente em **Linux (Pop!_OS 24.04, COSMIC) + GitHub (`gh`) + Claude Code**, e esse é o
cenário que mais importa não quebrar. Mudanças nas integrações com o GitHub e o Claude (`gh.rs`, `claude.rs`,
`suggest.rs`, `agents.rs`, `usage.rs`, `src/lib/github.ts`) precisam vir com teste e com descrição de como você
testou no app.

## Ambiente

Você precisa de **Node 24+** e de uma das opções abaixo pra compilar o Rust:

- **Docker** (recomendado, não instala nada na máquina): `docker compose run --rm build`
- **Nativo:** Rust via [rustup](https://rustup.rs) + bibliotecas do Tauri:
  ```sh
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

```sh
npm install
npm run tauri dev        # nativo: abre o app com hot reload
```

## Testando

1. `npm test`: testes do frontend (Vitest).
2. `npm run build`: checa os tipos e compila o frontend.
3. `docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo check && cargo test"`: Rust, **sem warnings**.
4. `docker compose run --rm build`: gera o `.deb` em `dist-deb/`.
5. `scripts/install-local.sh`: instala no seu usuário (sem sudo) e coloca o ícone no lançador. Teste no app
   instalado (`~/.local/bin/term7`).

> Rodar a interface dentro do Docker funciona (`docker compose run --rm dev`), mas sem GPU fica lento.
> Pra avaliar desempenho e interação, use o app instalado.

**Testes novos:** lógica nova ou parser novo vem com teste. No Rust, no `mod tests` do próprio arquivo, com
dados no formato real (anonimizados). No frontend, num `*.test.ts` ao lado do módulo.

## Commits

Usamos [Conventional Commits](https://www.conventionalcommits.org/pt-br/). A versão sai automaticamente deles:

| Prefixo                           | Efeito na versão |
|-----------------------------------|------------------|
| `fix:`                            | patch (1.0.**1**) |
| `feat:`                           | minor (1.**1**.0) |
| `feat!:` ou `BREAKING CHANGE:`    | major (**2**.0.0) |
| `docs:`, `chore:`, `refactor:`, `style:`, `test:`, `ci:` | sem release |

Exemplo: `feat: abrir arquivo do diff no visualizador`

## Pull requests

1. Crie uma branch a partir da `dev` (`feat/...`, `fix/...`).
2. Garanta que os testes, o `npm run build` e o `cargo check` passam **sem warnings**.
3. Abra o PR **para a `dev`** e preencha o modelo: o que mudou, como testar e prints em mudanças visuais.
   O CI roda os testes e compila o `.deb` no PR, como check; nada é publicado.

### Branches e releases

- **`dev`**: integração. Os PRs de feature e correção entram aqui. Merge na `dev` não gera release.
- **`main`**: só recebe o PR `dev → main`. Cada merge nela gera uma release com tudo que acumulou na `dev`:
  o [semantic-release](https://semantic-release.gitbook.io) calcula a versão, o CI compila o `.deb`, anexa na
  release e mantém só as 3 últimas.
- **A `dev` nunca é apagada.** No merge do PR `dev → main`, não clique em "Delete branch".

A versão vem só da release: o workflow grava ela no `package.json` antes de compilar. No repositório o
`package.json` fica em `0.0.0-dev`, então um build local aparece como `dev`.

## Estilo

- Textos da interface, comentários e docs internas em **português do Brasil**; nomes no código em inglês.
- Comentários curtos que explicam o *porquê*.
- Veja o [CLAUDE.md](CLAUDE.md) pra estrutura do projeto e as convenções técnicas.

## Licença

Ao contribuir, você concorda que sua contribuição seja distribuída sob a [licença MIT](LICENSE) do projeto.
