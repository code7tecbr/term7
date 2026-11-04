# Security / Segurança

## Reporting / Como relatar

Please **don't open a public issue** for security problems. Report them privately through
[GitHub's private vulnerability reporting](https://github.com/code7tecbr/term7/security/advisories/new),
with the steps to reproduce and the affected version. We'll reply as soon as possible and credit you in the
fix, if you want.

Por favor, **não abra issue pública** pra falha de segurança. Relate em particular pelo
[relato privado de vulnerabilidade do GitHub](https://github.com/code7tecbr/term7/security/advisories/new),
com os passos pra reproduzir e a versão afetada. Respondemos o quanto antes e damos o crédito na correção, se
você quiser.

## Supported versions / Versões com suporte

Only the latest release gets fixes. / Só a última release recebe correções.

## What term7 handles / O que o term7 manipula

- Runs your shell and programs with your user's permissions; it has no server and opens no network ports.
- Calls `git` and `gh` locally; GitHub credentials stay with `gh`.
- Reads Claude Code's local files (sessions, transcripts, settings) to show status, and can add hooks and a
  status line to Claude's `settings.json` when you enable them.
- The transcription API key is stored in `~/.local/share/br.tec.code7.term7/settings.json` with mode 600 and
  is never sent back to the UI.
- Dependency vulnerabilities are tracked by Dependabot.

Reports about any of these are in scope. / Relatos sobre qualquer um desses pontos estão no escopo.
