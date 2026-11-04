---
name: term7-contribute
description: >
  The term7 contribution flow: which branch to start from, how to name branches, Conventional Commit
  messages and how they drive semantic-release versions, and how to open the pull request (target dev,
  fill the PR template). Use whenever you are about to create a branch, write a commit message, commit,
  push, open or describe a PR, or the user asks how to contribute or release — even for a one-line fix.
  Gatilhos em português: "cria uma branch", "commita", "mensagem de commit", "abre o PR", "manda o PR",
  "como contribuir", "qual branch", "vai gerar release?".
---

# Contribuindo com o term7

Commit e push só quando o usuário pedir. Este guia diz *como*, não autoriza fazer.

## Branch

- Parta sempre da `dev` atualizada: `git fetch origin && git switch -c feat/<assunto> origin/dev`.
- Nomes: `feat/...`, `fix/...` (também `docs/...`, `chore/...`, `refactor/...`), curtos e em português,
  como `fix/comentar-trecho-removido`.
- **Nunca commite direto na `dev` nem na `main`**, e nunca apague a `dev` (nem com `--delete-branch`).

## Commits

[Conventional Commits](https://www.conventionalcommits.org/pt-br/): o semantic-release lê as mensagens
da `main` e decide a versão sozinho, então o prefixo errado gera a versão errada (ou nenhuma).

| Prefixo                                           | Efeito na versão  |
|---------------------------------------------------|-------------------|
| `fix:`                                            | patch (1.0.**1**) |
| `feat:`                                           | minor (1.**1**.0) |
| `feat!:` ou rodapé `BREAKING CHANGE:`             | major (**2**.0.0) |
| `docs:`, `chore:`, `refactor:`, `style:`, `test:`, `ci:` | sem release |

- Descrição em português, no imperativo, minúscula: `feat: abrir arquivo do diff no visualizador`.
- Escopo opcional entre parênteses: `fix(revisão): comentar trecho removido no modo alterações`.
- Use `fix:`/`feat:` só pro que o usuário do app percebe; ajuste interno é `refactor:`/`chore:`.

## Pull request

1. Antes de abrir, valide como descrito na skill `term7-dev` (`npm run build`, testes, `cargo check`
   sem warnings, teste no app instalado quando houver mudança de interface).
2. Abra o PR **para a `dev`**: `gh pr create --base dev`.
3. Preencha o template de PR de `.github/` (leia o arquivo e siga as seções dele; não substitua por um
   texto livre). Diga o que mudou e como testar; prints ou GIFs em mudança visual.
4. No PR o CI só compila o `.deb`, como check. Nada é publicado.

## Releases

- A `main` só recebe o PR `dev → main`. Cada merge nela gera release: o CI calcula a versão, compila
  o `.deb`, anexa na release e apaga as releases além das 3 mais recentes.
- Merge na `dev` não gera release; tudo acumula até o próximo `dev → main`.
