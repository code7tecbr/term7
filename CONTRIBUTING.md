# Contributing to term7

**English** · [Português](CONTRIBUTING.pt-BR.md)

Thanks for your interest! This guide covers reporting a problem, setting up the environment, testing and
sending your change.

## Issues

1. [Search the issues](https://github.com/code7tecbr/term7/issues?q=is%3Aissue) first: it may already exist.
2. Open one with the [right template](https://github.com/code7tecbr/term7/issues/new/choose): **bug** or
   **idea**. Blank issues are disabled so every report comes with the information we need.
3. For bugs, include the term7 version (right corner of the status bar), your distro and desktop and, for the
   integrations, your `gh` and `claude` versions. A screenshot or GIF helps a lot.
4. Security issues do **not** go in issues: see [SECURITY.md](SECURITY.md).

English or Portuguese are both fine.

### What the maintainer uses every day

term7 is used daily on **Linux (Pop!_OS 24.04, COSMIC) + GitHub (`gh`) + Claude Code**, and that is the setup
that matters most not to break. Changes to the GitHub and Claude integrations (`gh.rs`, `claude.rs`,
`suggest.rs`, `agents.rs`, `usage.rs`, `src/lib/github.ts`) need tests and a description of how you tested them
in the app.

## Environment

You need **Node 24+** and one of these to build the Rust side:

- **Docker** (recommended, installs nothing on your machine): `docker compose run --rm build`
- **Native:** Rust via [rustup](https://rustup.rs) + the Tauri libraries:
  ```sh
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

```sh
npm install
npm run tauri dev        # native: opens the app with hot reload
```

## Testing

1. `npm test`: frontend tests (Vitest).
2. `npm run build`: type-check and frontend build.
3. `docker compose run --rm --no-deps dev bash -c "cd src-tauri && cargo check && cargo test"`: Rust, **no warnings**.
4. `docker compose run --rm build`: builds the `.deb` into `dist-deb/`.
5. `scripts/install-local.sh`: installs it for your user (no sudo) with a launcher icon. Test the installed app
   (`~/.local/bin/term7`).

> Running the UI inside Docker works (`docker compose run --rm dev`), but without a GPU it is slow.
> To judge performance and interaction, use the installed app.

**New tests:** new logic or a new parser comes with tests. In Rust, in the file's own `mod tests`, with
real-shaped (anonymized) data. In the frontend, in a `*.test.ts` next to the module.

## Commits

We use [Conventional Commits](https://www.conventionalcommits.org/). The version is computed from them:

| Prefix                            | Version bump |
|-----------------------------------|--------------|
| `fix:`                            | patch (1.0.**1**) |
| `feat:`                           | minor (1.**1**.0) |
| `feat!:` or `BREAKING CHANGE:`    | major (**2**.0.0) |
| `docs:`, `chore:`, `refactor:`, `style:`, `test:`, `ci:` | no release |

Example: `feat: open diff file in the viewer`

## Pull requests

1. Branch off `dev` (`feat/...`, `fix/...`).
2. Make sure the tests, `npm run build` and `cargo check` pass **with no warnings**.
3. Open the PR **against `dev`** and fill in the template: what changed, how to test, and screenshots for
   visual changes. CI runs the tests and builds the `.deb` on the PR as a check; nothing is published.

### Branches and releases

- **`dev`**: integration. Feature and fix PRs land here. Merging into `dev` does not release.
- **`main`**: only takes the `dev → main` PR. Each merge releases everything accumulated on `dev`:
  [semantic-release](https://semantic-release.gitbook.io) computes the version, CI builds the `.deb`, attaches
  it to the release and keeps only the 3 latest.
- **`dev` is never deleted.** When merging `dev → main`, don't click "Delete branch".

The version only comes from the release: the workflow writes it into `package.json` before building. In the
repo `package.json` stays at `0.0.0-dev`, so a local build shows up as `dev`.

## Style

- UI text, code comments and internal docs are in **Brazilian Portuguese**; identifiers in English. If you
  don't write Portuguese, write in English and a maintainer will help.
- Short comments that explain *why*.
- See [CLAUDE.md](CLAUDE.md) for the project structure and technical conventions.

## License

By contributing, you agree that your contribution is distributed under the project's [MIT license](LICENSE).
