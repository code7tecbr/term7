# Extensões do term7

[English](README.md)

Complementos que rodam **fora** do term7, cada um no próprio processo, e falam com ele por uma API local. Se uma
extensão travar ou tiver bug, o terminal segue normal. Dá pra escrever extensão em qualquer linguagem.

| Extensão      | O que faz                                                                          |
|---------------|------------------------------------------------------------------------------------|
| [`mcp/`](#mcp) | Servidor MCP: o Claude Code lista as abas, lê a tela e manda mensagem pra outra aba. |

## Ligando a API

A API vem **desligada**. Ligue em **Configurações > Extensões**. Ela é um socket Unix em
`$XDG_RUNTIME_DIR/term7/api.sock`, com permissão só pro seu usuário. Só uma janela do term7 fica com a API por vez.

## MCP

Deixa o Claude Code de qualquer aba perguntar coisas como "tem alguma aba esperando por mim?" ou "avisa a aba da
branch `feat/x` que chegou review no PR".

Precisa do `python3` (já vem no Ubuntu) e do Claude Code. Instale com:

```sh
extensions/mcp/install.sh
```

O script copia `term7-mcp` pra `~/.local/bin` e roda `claude mcp add -s user term7`. Se você usa mais de uma conta do
Claude, rode de novo com o `CLAUDE_CONFIG_DIR` da outra conta:

```sh
CLAUDE_CONFIG_DIR=~/.claude-empresa extensions/mcp/install.sh
```

Ferramentas:

- `list_tabs`: abas com título, pasta, branch, tag e estado do Claude (trabalhando, esperando você, terminou).
- `read_tab`: últimas linhas da tela de uma aba.
- `send_to_tab`: manda uma mensagem pro Claude de outra aba, com Enter. Nunca vai pro shell: a aba precisa estar com
  o Claude aberto. Se ele estiver trabalhando ou esperando permissão, a mensagem entra numa fila e sai quando ele
  terminar (assim o Enter não responde uma permissão por engano). O term7 mostra um aviso quando uma extensão manda
  mensagem.

## Protocolo da API

Uma conexão pode fazer vários pedidos. Cada pedido é uma linha JSON e cada resposta também:

```json
{"method": "tabs.send", "params": {"tab": 3, "text": "Chegou review no PR #97", "pane": "<TERM7_PANE>"}}
{"result": {"status": "queued", "position": 1}}
{"error": "a aba \"x\" não está com o Claude aberto"}
```

`pane` é opcional: repasse o `TERM7_PANE` que o shell da aba exporta, pra o term7 saber de qual aba veio o pedido.

| Método       | Params                    | Resultado                                                                 |
|--------------|---------------------------|---------------------------------------------------------------------------|
| `version`    | —                         | `{"api": 1, "app": "1.2.3"}`                                              |
| `tabs.list`  | —                         | lista de `{id, title, cwd, branch, process, claude, attention, tag, paused, active, self, queued}` |
| `tabs.read`  | `tab`, `lines` (50, máx. 500) | `{id, lines: [...]}`                                                  |
| `tabs.send`  | `tab`, `text`             | `{"status": "sent"}` ou `{"status": "queued", "position": n}`             |

`claude` é `working`, `waiting`, `done`, `idle` (aberto e parado) ou `null` (sem Claude na aba). `api` muda quando
algum método muda de forma incompatível.

## Testes

```sh
python3 -m unittest discover -s extensions/mcp
```
