#!/bin/sh
set -eu
umask 077
DATA_DIR=${BE6500PANEL_DATA_DIR:-/data/be6500panel}
RUN_DIR=${BE6500PANEL_RUN_DIR:-/tmp/be6500panel}
ARCHIVE="$DATA_DIR/panel.tar.gz"
PID_FILE="$RUN_DIR/panel.pid"
mkdir -p "$RUN_DIR"
exec 9>"$RUN_DIR/.bootstrap.lock"
flock -n 9 || exit 0
if [ -f "$PID_FILE" ]; then
    pid=$(cat "$PID_FILE")
    if [ -r "/proc/$pid/comm" ]; then
        case "$(cat "/proc/$pid/comm")" in be6500-panel) exit 0;; esac
    fi
fi
[ -s "$ARCHIVE" ] || exit 1
[ -s "$DATA_DIR/panel.sha256" ] || exit 1
expected=$(cat "$DATA_DIR/panel.sha256")
printf '%s  %s\n' "$expected" "$ARCHIVE" | sha256sum -c - >/dev/null
candidate="$RUN_DIR/boot-release"
mkdir -p "$candidate"
tar -xzf "$ARCHIVE" -C "$candidate"
chmod 700 "$candidate/be6500-panel"
BE6500PANEL_PASSWORD=$(cat "$DATA_DIR/panel-password")
export BE6500PANEL_PASSWORD
if [ -d "$DATA_DIR/xtables" ]; then
    XTABLES_LIBDIR="$DATA_DIR/xtables"
    export XTABLES_LIBDIR
fi
# Release reconstruction preserves one volatile native artifact tree.
# Native restart rebuilding uses stored verified artifact request on later boots.
[ -s "$DATA_DIR/native-command-base.json" ] || exit 1
cp "$DATA_DIR/native-command-base.json" "$RUN_DIR/native-bindings.json"
if [ -f "$RUN_DIR/native-run/sing-box/.artifact-release" ]; then
    [ -s "$DATA_DIR/native-handover-bindings.json" ] || exit 1
    cp "$DATA_DIR/native-handover-bindings.json" "$RUN_DIR/native-bindings.json"
fi
chmod 600 "$RUN_DIR/native-bindings.json"
mkdir -p "$RUN_DIR/native-run"
chmod 700 "$RUN_DIR/native-run"
# Use the LAN bridge's current address, including upstream DHCP in AP mode.
lan_ip=$(/usr/sbin/ip -4 -o address show dev br-lan scope global | awk '{split($4,a,"/"); print a[1]; exit}')
[ -n "$lan_ip" ] || lan_ip=$(/sbin/uci -q get network.lan.ipaddr)
[ -n "$lan_ip" ] || exit 1
trap '' HUP
"$candidate/be6500-panel" --native-runtime --listen "$lan_ip:8787" --web-dir "$candidate/web" --data-dir "$DATA_DIR" --run-dir "$RUN_DIR/native-run" --command-manifest "$RUN_DIR/native-bindings.json" 9>&- </dev/null >"$RUN_DIR/panel.log" 2>&1 &
printf '%s\n' "$!" >"$PID_FILE"
