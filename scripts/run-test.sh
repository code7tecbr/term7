#!/usr/bin/env bash
# Abre a build de teste (term7-dev) sem instalar nada: extrai o .deb de dist-test/
# em dist-test/root e roda de lá. A release instalada continua intacta e as duas
# rodam juntas (identificadores diferentes: configurações, sessão e janela à parte).
# Uso: docker compose run --rm build-test && scripts/run-test.sh
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)/dist-test"
DEB=$(ls -t "$DIR"/*.deb 2>/dev/null | head -1)
[[ -n "$DEB" ]] || { echo "Nenhum .deb em dist-test/. Rode: docker compose run --rm build-test" >&2; exit 1; }

rm -rf "$DIR/root"
dpkg-deb -x "$DEB" "$DIR/root"
BIN=$(find "$DIR/root/usr/bin" -maxdepth 1 -type f -name 'term7*' | head -1)
echo "Abrindo $BIN"
# Rodado de dentro do Claude Code (`! scripts/run-test.sh`), não leva as marcas da sessão dele.
for var in $(compgen -e | grep -E '^CLAUDE(CODE$|_CODE_|_PID$|_EFFORT$)' | grep -v '^CLAUDE_CODE_ENABLE_TELEMETRY$'); do
  unset "$var"
done
setsid -f "$BIN" >/dev/null 2>&1
