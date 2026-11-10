#!/bin/sh
# Instala o MCP do term7 em ~/.local/bin e registra no Claude Code (escopo do usuário).
# Pra outra conta do Claude, rode de novo com o CLAUDE_CONFIG_DIR dela:
#   CLAUDE_CONFIG_DIR=~/.claude-empresa ./install.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin/term7-mcp"

command -v python3 >/dev/null || { echo "Precisa do python3." >&2; exit 1; }
command -v claude >/dev/null || { echo "Precisa do Claude Code (comando claude) no PATH." >&2; exit 1; }

mkdir -p "$HOME/.local/bin"
install -m 755 "$here/term7-mcp" "$bin"

# Reinstalar troca o registro antigo.
claude mcp remove -s user term7 >/dev/null 2>&1 || true
claude mcp add -s user term7 -- "$bin"

echo "MCP do term7 instalado em $bin."
echo "Ligue a API no term7 (Configurações > Extensões) e reinicie o Claude nas abas."
