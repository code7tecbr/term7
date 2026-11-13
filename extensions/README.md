# term7 extensions

[Português](README.pt-BR.md)

Add-ons that run **outside** term7, each in its own process, and talk to it through a local API. If an extension
hangs or misbehaves, the terminal keeps working. Extensions can be written in any language.

| Extension      | What it does                                                                     |
|----------------|----------------------------------------------------------------------------------|
| [`mcp/`](#mcp) | MCP server: Claude Code lists your tabs, reads their screen and messages another tab. |
| [`statusbot/`](#statusbot) | Shows in the status bar what [statusbot](https://github.com/wisley7l/statusbot) is watching. |

## Installing

Extensions ship with term7. Open **Settings > Extensions**, click an extension, fill in the fields and click
**Install**. Installing turns the API on. To change the configuration, edit the fields and click **Save**; to take it
out, **Remove**. Tokens are kept in `~/.config/term7/<id>.env` (mode 600) and never come back to the screen.

Each folder's `install.sh` does the same from a terminal, using the repository.

## Turning the API on

The API is **off** by default. Turn it on in **Settings > Extensions**. It is a Unix socket at
`$XDG_RUNTIME_DIR/term7/api.sock`, readable only by your user. Only one term7 window holds the API at a time.

## MCP

Lets Claude Code in any tab ask things like "is any tab waiting for me?" or "tell the tab on branch `feat/x` that
the PR got a review".

Needs `python3` (already on Ubuntu) and Claude Code. On the screen, tick the Claude accounts to install it in: term7
finds the default one (`~/.claude.json`) and every `~/.claude-*` with a `.claude.json` inside. From a terminal:

```sh
extensions/mcp/install.sh
```

The script copies `term7-mcp` to `~/.local/bin` and runs `claude mcp add -s user term7`. If you use more than one
Claude account, run it again with the other account's `CLAUDE_CONFIG_DIR`:

```sh
CLAUDE_CONFIG_DIR=~/.claude-work extensions/mcp/install.sh
```

Tools:

- `list_tabs`: tabs with title, folder, branch, tag and Claude state (working, waiting for you, done).
- `read_tab`: the last lines on a tab's screen.
- `send_to_tab`: sends a message to the Claude in another tab, pressing Enter. It never goes to a shell: the tab must
  have Claude open. If Claude is working or waiting for a permission, the message is queued and sent when it is done
  (so Enter never answers a permission by mistake). term7 shows a notice whenever an extension sends a message.
- `service_status`: the status of services published by extensions (such as statusbot), so Claude can tell whether
  an error comes from a service that is down.

## statusbot

Adds a button to term7's status bar with the state of [statusbot](https://github.com/wisley7l/statusbot): green,
yellow or red, with how many services are not normal. Click it for the list, worst first. Works with statusbot in a
local Docker container or on a server.

Needs statusbot with its HTTP endpoint on (`HTTP_PORT`; on a server, also `HTTP_TOKEN`). On the screen, fill in the
URL and token. From a terminal, the script asks for the token without echoing it:

```sh
extensions/statusbot/install.sh http://localhost:8080
```

The script copies `term7-statusbot` to `~/.local/bin`, writes the config to `~/.config/term7/statusbot.env`
(mode 600) and creates the `term7-statusbot` user service (systemd), which starts with your session and reads
statusbot every minute. While term7 is closed, it just tries again on the next round.

- Logs: `journalctl --user -u term7-statusbot -f`
- Run once: `STATUSBOT_URL=http://localhost:8080 ~/.local/bin/term7-statusbot --once`
- Remove: `systemctl --user disable --now term7-statusbot`, then delete `~/.local/bin/term7-statusbot`,
  `~/.config/term7/statusbot.env` and `~/.config/systemd/user/term7-statusbot.service`.

## Local voice (Kokoro)

Reads Claude's replies aloud with [Kokoro-82M](https://github.com/thewh1teagle/kokoro-onnx), generated on your own
machine: no internet, no quota, and the text never leaves the computer. Unlike the others, this extension doesn't use
the API: term7 opens it once per reading (`--serve`), so the model loads once and the next chunks are generated
while the current one plays.

Install it from the screen (Settings → Extensions → Local voice). Installing creates a venv in
`~/.local/share/term7/kokoro` with `kokoro-onnx` and downloads the int8 model (~120 MB). Then, in Settings → Read
aloud, pick “Local voice (extension)”.

- **Voice and language:** `pm_alex` (default), `pm_santa` and `pf_dora` speak Brazilian Portuguese; with `en-us`,
  the English voices (`af_heart`, `am_michael`…).
- **Speed:** 1.0 is normal; 0.8 speaks slower.
- **CPU cores:** how many to use while generating (default 4). Each reply read aloud keeps the CPU busy for a few
  seconds. With 1 core generation barely keeps up with speech; use 2 or more if you hear pauses.
- Try it outside term7: `echo "Hello" | python3 /usr/share/term7/extensions/kokoro/term7-kokoro /tmp/hello.wav`
- Remove: from the screen, or delete `~/.local/share/term7/kokoro` and `~/.config/term7/kokoro.env`.

## API protocol

A connection can make several requests. Each request is one JSON line, and so is each reply:

```json
{"method": "tabs.send", "params": {"tab": 3, "text": "PR #97 got a review", "pane": "<TERM7_PANE>"}}
{"result": {"status": "queued", "position": 1}}
{"error": "a aba \"x\" não está com o Claude aberto"}
```

`pane` is optional: pass along the `TERM7_PANE` exported by the tab's shell, so term7 knows which tab is asking.

| Method       | Params                     | Result                                                                   |
|--------------|----------------------------|--------------------------------------------------------------------------|
| `version`    | —                          | `{"api": 1, "app": "1.2.3"}`                                             |
| `tabs.list`  | —                          | list of `{id, title, cwd, branch, process, claude, attention, tag, paused, active, self, queued}` |
| `tabs.read`  | `tab`, `lines` (50, max 500) | `{id, lines: [...]}`                                                   |
| `tabs.send`  | `tab`, `text`              | `{"status": "sent"}` or `{"status": "queued", "position": n}`            |
| `status.set` | `source`, `title`, `items`, `checkedAt`, `error`, `link`, `ttlSeconds` | `{"ok": true}`       |
| `status.clear` | `source`                 | `{"ok": true}`                                                           |
| `status.get` | —                          | what was published, with `stale`                                         |

`claude` is `working`, `waiting`, `done`, `idle` (open and idle) or `null` (no Claude in the tab). `api` changes when
a method changes in an incompatible way. Error messages are in Portuguese.

With `status.set`, each extension publishes under its own `source` (e.g. `statusbot`), and each one becomes a button
in the status bar. `items` is a list of `{label, status, since?, detail?, link?}`, where `status` is `up`, `degraded`,
`maintenance`, `down` or `unknown`. `error` shows a warning (e.g. the service is unreachable). Without a new
`status.set` within `ttlSeconds` (default 300), the button turns grey: the extension stopped. Only http(s) links open.

## Writing an extension

Each folder has an `extension.json`, which the screen uses to build the form and install:

```json
{
  "id": "statusbot",
  "name": "statusbot",
  "description": "What it does, in one sentence.",
  "link": "https://...",
  "kind": "service",
  "entry": "term7-statusbot",
  "config": [
    { "key": "STATUSBOT_URL", "label": "URL", "type": "url", "required": true, "placeholder": "http://localhost:8080" },
    { "key": "STATUSBOT_TOKEN", "label": "Token", "type": "secret" }
  ]
}
```

- `kind`: `service` becomes a systemd user service (`term7-<id>`), with `config` as environment variables; `mcp` is
  registered with `claude mcp add-json -s user` in the chosen accounts.
- `entry`: the folder's Python script (run with `python3`).
- Field `type`: `text`, `url`, `secret` or `number`.
- New files go into the `.deb` through `bundle.linux.deb.files` in `src-tauri/tauri.conf.json`.

## Tests

```sh
python3 -m unittest discover -s extensions/mcp
python3 -m unittest discover -s extensions/statusbot
python3 -m unittest discover -s extensions/kokoro
```
