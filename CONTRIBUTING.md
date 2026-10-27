# Contribuindo com o term7

Valeu pelo interesse! Este guia mostra como montar o ambiente, testar e mandar sua mudança.

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

1. `npm run build`: checa os tipos e compila o frontend.
2. `docker compose run --rm build`: gera o `.deb` em `dist-deb/`.
3. `scripts/install-local.sh`: instala no seu usuário (sem sudo) e coloca o ícone no lançador.

> Rodar a interface dentro do Docker funciona (`docker compose run --rm dev`), mas sem GPU fica lento.
> Pra avaliar desempenho, use o app instalado.

## Commits

Usamos [Conventional Commits](https://www.conventionalcommits.org/pt-br/). A versão sai automaticamente deles:

| Prefixo                           | Efeito na versão |
|-----------------------------------|------------------|
| `fix:`                            | patch (1.0.**1**) |
| `feat:`                           | minor (1.**1**.0) |
| `feat!:` ou `BREAKING CHANGE:`    | major (**2**.0.0) |
| `docs:`, `chore:`, `refactor:`, `style:`, `test:` | sem release |

Exemplo: `feat: abrir arquivo do diff no visualizador`

## Pull requests

1. Crie uma branch a partir da `main` (`feat/...`, `fix/...`).
2. Garanta que `npm run build` e o `cargo check` passam **sem warnings**.
3. Abra o PR descrevendo o que mudou e como testar. Prints ou GIFs ajudam muito em mudanças visuais.
4. Ao mergear na `main`, o CI cria a release com o `.deb` e mantém só as 3 últimas.

## Estilo

- Textos da interface, comentários e docs em **português do Brasil**; nomes no código em inglês.
- Comentários curtos que explicam o *porquê*.
- Veja o [CLAUDE.md](CLAUDE.md) pra estrutura do projeto e as convenções técnicas.
