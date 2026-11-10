#!/usr/bin/env bash
# Instala o term7 só pro usuário atual (sem sudo), a partir do .deb em dist-deb/.
# Uso: scripts/install-local.sh            -> instala/atualiza
#      scripts/install-local.sh --remove   -> desinstala
set -euo pipefail

BIN="$HOME/.local/bin/term7"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor"
DESKTOP="$APPS/br.tec.code7.term7.desktop"
EXTENSIONS="$HOME/.local/share/term7/extensions"

if [[ "${1:-}" == "--remove" ]]; then
  rm -f "$BIN" "$DESKTOP"
  rm -rf "$EXTENSIONS"
  find "$ICONS" -name term7.png -path "*/apps/*" -delete 2>/dev/null || true
  echo "term7 removido."
  exit 0
fi

DEB=$(ls -t "$(dirname "$0")"/../dist-deb/term7_*.deb 2>/dev/null | head -1)
[[ -n "$DEB" ]] || { echo "Nenhum .deb em dist-deb/. Rode: docker compose run --rm build" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
dpkg-deb -x "$DEB" "$TMP"

install -Dm755 "$TMP/usr/bin/term7" "$BIN"
# Extensões ao lado, como no pacote (o app procura em <prefixo>/share/term7/extensions).
rm -rf "$EXTENSIONS"
mkdir -p "$(dirname "$EXTENSIONS")"
cp -r "$TMP/usr/share/term7/extensions" "$EXTENSIONS"

# Sem passar pelo apt, as dependências do .deb não são instaladas: só avisa.
for cmd in git gh; do
  command -v "$cmd" >/dev/null || echo "Aviso: '$cmd' não encontrado. Instale com: sudo apt install $cmd" >&2
done
for dir in "$TMP"/usr/share/icons/hicolor/*/apps; do
  size=$(basename "$(dirname "$dir")")
  install -Dm644 "$dir/term7.png" "$ICONS/$size/apps/term7.png"
done
# Caminho absoluto no Exec: o lançador nem sempre tem ~/.local/bin no PATH.
sed "s|^Exec=.*|Exec=$BIN|" "$TMP/usr/share/applications/term7.desktop" | install -Dm644 /dev/stdin "$DESKTOP"

update-desktop-database "$APPS" 2>/dev/null || true
gtk-update-icon-cache -q "$ICONS" 2>/dev/null || true
echo "term7 instalado: $BIN"
