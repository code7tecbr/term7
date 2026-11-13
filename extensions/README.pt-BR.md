# Extensões do term7

[English](README.md)

Complementos que rodam **fora** do term7, cada um no próprio processo, e falam com ele por uma API local. Se uma
extensão travar ou tiver bug, o terminal segue normal. Dá pra escrever extensão em qualquer linguagem.

| Extensão      | O que faz                                                                          |
|---------------|------------------------------------------------------------------------------------|
| [`mcp/`](#mcp) | Servidor MCP: o Claude Code lista as abas, lê a tela e manda mensagem pra outra aba. |
| [`statusbot/`](#statusbot) | Mostra na barra de status o que o [statusbot](https://github.com/wisley7l/statusbot) está monitorando. |

## Instalando

As extensões vêm junto com o term7. Abra **Configurações > Extensões**, clique na extensão, preencha os campos e
clique em **Instalar**. Instalar liga a API sozinho. Pra mudar a configuração, mude os campos e clique em **Salvar**;
pra tirar, **Remover**. Os tokens ficam em `~/.config/term7/<id>.env` (permissão 600) e não voltam pra tela.

Os scripts `install.sh` de cada pasta fazem o mesmo pelo terminal, a partir do repositório.

## Ligando a API

A API vem **desligada**. Ligue em **Configurações > Extensões**. Ela é um socket Unix em
`$XDG_RUNTIME_DIR/term7/api.sock`, com permissão só pro seu usuário. Só uma janela do term7 fica com a API por vez.

## MCP

Deixa o Claude Code de qualquer aba perguntar coisas como "tem alguma aba esperando por mim?" ou "avisa a aba da
branch `feat/x` que chegou review no PR".

Precisa do `python3` (já vem no Ubuntu) e do Claude Code. Na tela, marque as contas do Claude onde instalar: o term7
acha a padrão (`~/.claude.json`) e cada `~/.claude-*` com um `.claude.json` dentro. Pelo terminal:

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
- `service_status`: status dos serviços publicados por extensões (como o statusbot), pra o Claude saber se um erro
  é um serviço fora do ar.

## statusbot

Mostra na barra de status do term7 um botão com o estado do [statusbot](https://github.com/wisley7l/statusbot):
verde, amarelo ou vermelho, com o número de serviços fora do normal. Ao clicar, aparece a lista, com o pior primeiro.
Funciona com o statusbot em Docker local ou num servidor.

Precisa do statusbot com o endpoint HTTP ligado (`HTTP_PORT`; num servidor, também `HTTP_TOKEN`). Na tela, informe a
URL e o token. Pelo terminal, o script pede o token sem mostrar o que você digita:

```sh
extensions/statusbot/install.sh http://localhost:8080
```

O script copia `term7-statusbot` pra `~/.local/bin`, grava a configuração em `~/.config/term7/statusbot.env`
(permissão 600) e cria o serviço do usuário `term7-statusbot` (systemd), que sobe junto com a sessão e lê o statusbot
a cada minuto. Com o term7 fechado, ele só tenta de novo na volta seguinte.

- Logs: `journalctl --user -u term7-statusbot -f`
- Testar uma vez: `STATUSBOT_URL=http://localhost:8080 ~/.local/bin/term7-statusbot --once`
- Remover: `systemctl --user disable --now term7-statusbot` e apagar `~/.local/bin/term7-statusbot`,
  `~/.config/term7/statusbot.env` e `~/.config/systemd/user/term7-statusbot.service`.

## Voz local (Kokoro)

Lê as respostas do Claude em voz alta com o [Kokoro-82M](https://github.com/thewh1teagle/kokoro-onnx), gerado na
própria máquina: sem internet, sem cota e sem o texto sair do computador. Diferente das outras, essa extensão não usa a
API: o term7 abre ela uma vez por leitura (`--serve`), então o modelo carrega uma vez só e os próximos trechos são
gerados enquanto o atual toca.

Instale pela tela (Configurações → Extensões → Voz local). A instalação cria um venv em `~/.local/share/term7/kokoro`
com o `kokoro-onnx` e baixa o modelo int8 (~120 MB). Depois, em Configurações → Leitura em voz alta, escolha
“Voz local (extensão)”.

- **Voz e idioma:** `pm_alex` (padrão), `pm_santa` e `pf_dora` falam pt-BR; com `en-us`, as vozes em inglês
  (`af_heart`, `am_michael`…).
- **Velocidade:** 1.0 é o normal; 0.8 fala mais devagar.
- **Núcleos da CPU:** quantos usar pra gerar (padrão 4). Cada resposta lida ocupa a CPU por alguns segundos.
  Com 1 núcleo a geração fica quase no ritmo da fala; use 2 ou mais se ouvir pausas.
- Testar fora do term7: `echo "Olá" | python3 /usr/share/term7/extensions/kokoro/term7-kokoro /tmp/ola.wav`
- Remover: pela tela, ou apagar `~/.local/share/term7/kokoro` e `~/.config/term7/kokoro.env`.

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
| `status.set` | `source`, `title`, `items`, `checkedAt`, `error`, `link`, `ttlSeconds` | `{"ok": true}`        |
| `status.clear` | `source`                | `{"ok": true}`                                                            |
| `status.get` | —                         | lista do que foi publicado, com `stale`                                   |

`claude` é `working`, `waiting`, `done`, `idle` (aberto e parado) ou `null` (sem Claude na aba). `api` muda quando
algum método muda de forma incompatível.

No `status.set`, cada extensão publica com um `source` próprio (ex.: `statusbot`), e cada um vira um botão na barra.
`items` é uma lista de `{label, status, since?, detail?, link?}`, com `status` igual a `up`, `degraded`,
`maintenance`, `down` ou `unknown`. `error` mostra um aviso (ex.: não alcançou o serviço). Sem um novo `status.set`
em `ttlSeconds` (padrão 300), o botão fica cinza: a extensão parou. Links só abrem se forem http(s).

## Escrevendo uma extensão

Cada pasta tem um `extension.json`, que a tela usa pra montar o formulário e instalar:

```json
{
  "id": "statusbot",
  "name": "statusbot",
  "description": "O que ela faz, numa frase.",
  "link": "https://...",
  "kind": "service",
  "entry": "term7-statusbot",
  "config": [
    { "key": "STATUSBOT_URL", "label": "URL", "type": "url", "required": true, "placeholder": "http://localhost:8080" },
    { "key": "STATUSBOT_TOKEN", "label": "Token", "type": "secret" }
  ]
}
```

- `kind`: `service` vira um serviço do usuário no systemd (`term7-<id>`), com a `config` como variáveis de ambiente;
  `mcp` é registrado com `claude mcp add-json -s user` nas contas escolhidas.
- `entry`: o script Python da pasta (roda com `python3`).
- `type` de cada campo: `text`, `url`, `secret` ou `number`.
- Novos arquivos entram no `.deb` pelo `bundle.linux.deb.files` do `src-tauri/tauri.conf.json`.

## Testes

```sh
python3 -m unittest discover -s extensions/mcp
python3 -m unittest discover -s extensions/statusbot
python3 -m unittest discover -s extensions/kokoro
```
