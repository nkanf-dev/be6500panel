#!/bin/sh
# Reconstruct and supervise the panel without replacing factory services.
set -eu
umask 077
DATA_DIR=${BE6500PANEL_DATA_DIR:-/data/be6500panel}
RUN_DIR=${BE6500PANEL_RUN_DIR:-/tmp/be6500panel}
ARCHIVE="$DATA_DIR/panel.tar.gz"
PID_FILE="$RUN_DIR/panel.pid"
mkdir -p "$RUN_DIR"
if [ -f "$PID_FILE" ]; then
    pid=$(cat "$PID_FILE")
    if [ -r "/proc/$pid/comm" ]; then
        case "$(cat "/proc/$pid/comm")" in be6500panel|panel-reload-*) exit 0;; esac
    fi
fi
[ -s "$ARCHIVE" ] || exit 1
[ -s "$DATA_DIR/panel.sha256" ] || exit 1
expected=$(cat "$DATA_DIR/panel.sha256")
printf '%s  %s\n' "$expected" "$ARCHIVE" | sha256sum -c - >/dev/null
candidate="$RUN_DIR/boot-release"
mkdir -p "$candidate"
tar -xzf "$ARCHIVE" -C "$candidate"
chmod 700 "$candidate/be6500panel"
BE6500PANEL_PASSWORD=$(cat "$DATA_DIR/panel-password")
export BE6500PANEL_PASSWORD
trap '' HUP
"$candidate/be6500panel" --listen 192.168.31.1:8787 --web-dir "$candidate/web" --data-dir "$DATA_DIR" --run-dir "$RUN_DIR/managed" --enable-control --artifact-transport curl </dev/null >"$RUN_DIR/panel.log" 2>&1 &
printf '%s\n' "$!" >"$PID_FILE"
