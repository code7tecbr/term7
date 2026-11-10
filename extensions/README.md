# term7 extensions

[Português](README.pt-BR.md)

Add-ons that run **outside** term7, each in its own process, and talk to it through a local API. If an extension
hangs or misbehaves, the terminal keeps working. Extensions can be written in any language.

| Extension      | What it does                                                                     |
|----------------|----------------------------------------------------------------------------------|
| [`mcp/`](#mcp) | MCP server: Claude Code lists your tabs, reads their screen and messages another tab. |

## Turning the API on

The API is **off** by default. Turn it on in **Settings > Extensions**. It is a Unix socket at
`$XDG_RUNTIME_DIR/term7/api.sock`, readable only by your user. Only one term7 window holds the API at a time.

## MCP

Lets Claude Code in any tab ask things like "is any tab waiting for me?" or "tell the tab on branch `feat/x` that
the PR got a review".

Needs `python3` (already on Ubuntu) and Claude Code. Install with:

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

`claude` is `working`, `waiting`, `done`, `idle` (open and idle) or `null` (no Claude in the tab). `api` changes when
a method changes in an incompatible way. Error messages are in Portuguese.

## Tests

```sh
python3 -m unittest discover -s extensions/mcp
```
