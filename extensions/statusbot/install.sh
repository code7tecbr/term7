#!/bin/sh
# Instala a extensão statusbot do term7 como serviço do usuário (systemd), que
# sobe junto com a sessão e manda o status pro term7 a cada minuto.
#   extensions/statusbot/install.sh http://localhost:8080
#   extensions/statusbot/install.sh https://statusbot.exemplo.com   (pede o token)
set -eu

url=${1:-}
[ -n "$url" ] || { echo "Uso: $0 <URL do statusbot>   (ex.: http://localhost:8080)" >&2; exit 1; }
command -v python3 >/dev/null || { echo "Precisa do python3." >&2; exit 1; }

here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin/term7-statusbot"
conf_dir="${XDG_CONFIG_HOME:-$HOME/.config}/term7"
env_file="$conf_dir/statusbot.env"
unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

mkdir -p "$HOME/.local/bin" "$conf_dir" "$unit_dir"
install -m 755 "$here/term7-statusbot" "$bin"

# Token lido sem eco (não fica no histórico do shell). Vazio = sem token (localhost).
printf "Token do statusbot (HTTP_TOKEN; Enter se não tiver): "
stty -echo 2>/dev/null || true
read -r token || token=""
stty echo 2>/dev/null || true
echo

umask 077
cat > "$env_file" <<CONF
STATUSBOT_URL=$url
STATUSBOT_TOKEN=$token
STATUSBOT_INTERVAL=60
CONF

cat > "$unit_dir/term7-statusbot.service" <<UNIT
[Unit]
Description=term7: status do statusbot na barra do term7

[Service]
EnvironmentFile=$env_file
ExecStart=$bin
Restart=on-failure
RestartSec=30

[Install]
WantedBy=default.target
UNIT

systemctl --user daemon-reload
systemctl --user enable --now term7-statusbot.service
systemctl --user restart term7-statusbot.service

echo "Pronto. Configuração em $env_file."
echo "Ligue a API no term7 (Configurações > Extensões) se ainda não ligou."
echo "Logs: journalctl --user -u term7-statusbot -f"
