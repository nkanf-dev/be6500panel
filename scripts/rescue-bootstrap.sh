#!/bin/sh
# /data survives the observed RN02 1.0.64 /etc rewrite. No promise is made
# for factory reset or a future update that erases /data or its cron trigger.
# Shared routines also serve setup and the single procd service.
if [ "${RESCUE_LIBRARY_ONLY:-0}" != 1 ]; then set -eu; fi
umask 077
DATA=/data/ssh
RUN=/tmp/be6500-rescue
PROC=/proc
INIT=/etc/init.d/be6500-rescue
NATIVE=/etc/dropbear
rescue_fail() { printf 'RESCUE_ERROR %s\n' "$1" >&2; exit 1; }
rescue_regular() { [ -f "$1" ] && [ ! -L "$1" ]; }
rescue_dirs() {
    local d
    for d in /data "$DATA" "$DATA/bin" "$RUN" "$NATIVE" /etc/init.d; do
        [ ! -L "$d" ] || rescue_fail unsafe-path
        [ ! -e "$d" ] || [ -d "$d" ] || rescue_fail unsafe-path
    done
    mkdir -p "$DATA/bin" "$RUN" "$NATIVE"
    chmod 700 "$DATA" "$DATA/bin" "$RUN"
}
rescue_lock() {
    [ ! -L "$RUN/bootstrap.lock" ] || rescue_fail unsafe-lock
    exec 9>"$RUN/bootstrap.lock"
    flock -n 9 || rescue_fail busy
}
rescue_digest() {
    local expected
    rescue_regular "$1" && rescue_regular "$1.sha256" || return 1
    [ -s "$1" ] || return 1
    expected=$(cat "$1.sha256")
    [ "${#expected}" -eq 64 ] || return 1
    case "$expected" in *[!0-9a-fA-F]*) return 1;; esac
    printf '%s  %s\n' "$expected" "$1" | sha256sum -c - >/dev/null 2>&1
}
rescue_validate() {
    rescue_digest "$DATA/bin/dropbear" || rescue_fail binary-integrity
    rescue_digest "$DATA/rescue-ssh.init" || rescue_fail init-integrity
    # Hashes detect corruption, not a malicious root or firmware replacement.
    rescue_regular "$DATA/dropbear_rsa_host_key" && [ -s "$DATA/dropbear_rsa_host_key" ] || rescue_fail host-key
    rescue_regular "$DATA/authorized_keys" && [ -s "$DATA/authorized_keys" ] || rescue_fail public-key
    chmod 700 "$DATA/bin/dropbear"
    chmod 600 "$DATA/dropbear_rsa_host_key" "$DATA/authorized_keys"
}
rescue_lan() {
    local iptool addresses
    [ -d /sys/class/net/br-lan/bridge ] || return 1
    iptool=$(command -v ip) || return 1
    addresses=$("$iptool" -4 -o address show dev br-lan scope global 2>/dev/null |
        awk '{split($4,a,"/"); print a[1]}') || return 1
    # An ambiguous bridge address is not a reason to bind a wildcard or WAN.
    [ "$(printf '%s\n' "$addresses" | wc -l | tr -d ' ')" = 1 ] || return 1
    printf '%s\n' "$addresses" | awk -F. '
      NF != 4 {exit 1}
      {for (i=1;i<=4;i++) if ($i !~ /^[0-9]+$/ || $i>255 || (length($i)>1 && substr($i,1,1)=="0")) exit 1}
      $1==0 || $1==127 || $1>=224 || ($1==169 && $2==254) {exit 1}
      {print}'
}
rescue_hex_ip() {
    printf '%s\n' "$1" | awk -F. '{printf "%02X%02X%02X%02X",$4,$3,$2,$1}'
}
rescue_port_busy() {
    local hex
    hex=$(printf '%04X' "$1")
    awk -v p="$hex" '$4=="0A" {split($2,a,":"); if (a[2]==p) found=1} END {exit !found}' "$PROC/net/tcp" "$PROC/net/tcp6"
}
rescue_owned_ready() {
    local pid lanhex inode fd link foundlan foundloop arg
    rescue_regular /var/run/be6500-rescue-2222.pid || return 1
    pid=$(cat /var/run/be6500-rescue-2222.pid)
    case "$pid" in ''|*[!0-9]*) return 1;; esac
    [ "$(readlink "$PROC/$pid/exe" 2>/dev/null)" = "$DATA/bin/dropbear" ] || return 1
    for arg in -s "$LAN:2222" 127.0.0.1:2222; do
        tr '\000' '\n' < "$PROC/$pid/cmdline" | grep -Fxq -- "$arg" || return 1
    done
    lanhex=$(rescue_hex_ip "$LAN")
    foundlan=0; foundloop=0
    for fd in "$PROC/$pid"/fd/*; do
        link=$(readlink "$fd" 2>/dev/null) || continue
        case "$link" in 'socket:['*']') inode=${link#socket:[}; inode=${inode%]};; *) continue;; esac
        if awk -v i="$inode" -v a="$lanhex:08AE" '$4=="0A" && $2==a && $10==i {f=1} END {exit !f}' "$PROC/net/tcp"; then foundlan=1; fi
        if awk -v i="$inode" '$4=="0A" && $2=="0100007F:08AE" && $10==i {f=1} END {exit !f}' "$PROC/net/tcp"; then foundloop=1; fi
        # No owned wildcard, IPv6, WAN, or stale LAN listener is accepted.
        if awk -v i="$inode" -v a="$lanhex" '$4=="0A" && $10==i && $2!=a":08AE" && $2!="0100007F:08AE" && $2!=a":0016" {bad=1} END {exit !bad}' "$PROC/net/tcp" "$PROC/net/tcp6"; then return 1; fi
    done
    [ "$foundlan" = 1 ] && [ "$foundloop" = 1 ] || return 1
    # A real bounded local SSH banner check. This is NOT client key-login proof.
    printf '\n' | nc -w 2 127.0.0.1 2222 2>/dev/null | grep -q '^SSH-2.0-'
}
rescue_restore_keys() {
    if [ -L "$NATIVE/authorized_keys" ]; then
        [ "$(readlink "$NATIVE/authorized_keys")" = "$DATA/authorized_keys" ] || rescue_fail native-key-path
    elif [ -e "$NATIVE/authorized_keys" ]; then
        rescue_regular "$NATIVE/authorized_keys" || rescue_fail native-key-path
        # Preserve unrelated factory/owner key rows, including key options.
        awk 'FNR==NR {print; seen[$0]=1; next} !seen[$0]++ {print}' "$DATA/authorized_keys" "$NATIVE/authorized_keys" > "$RUN/keys.next"
        cp "$RUN/keys.next" "$DATA/authorized_keys"
        cp "$DATA/authorized_keys" "$NATIVE/authorized_keys"
        chmod 600 "$NATIVE/authorized_keys"
        rm -f "$RUN/keys.next"
    else
        ln -s "$DATA/authorized_keys" "$NATIVE/authorized_keys"
    fi
    [ ! -L "$NATIVE/dropbear_rsa_host_key" ] || rescue_fail native-host-path
    if [ ! -e "$NATIVE/dropbear_rsa_host_key" ]; then
        cp "$DATA/dropbear_rsa_host_key" "$NATIVE/dropbear_rsa_host_key"
        chmod 600 "$NATIVE/dropbear_rsa_host_key"
    fi
}
rescue_reconcile() {
    local changed attempt
    rescue_validate
    LAN=$(rescue_lan) || rescue_fail lan-address
    rescue_restore_keys
    changed=0
    [ ! -L "$INIT" ] && [ ! -L "$INIT.next" ] || rescue_fail init-path
    if ! cmp -s "$DATA/rescue-ssh.init" "$INIT"; then
        cp "$DATA/rescue-ssh.init" "$INIT.next"
        chmod 700 "$INIT.next"
        mv "$INIT.next" "$INIT"
        changed=1
    fi
    "$INIT" enable >/dev/null 2>&1 || rescue_fail init-enable
    if [ "$changed" = 1 ] || ! rescue_owned_ready; then
        # procd replaces only this named service. Never background a new daemon.
        "$INIT" restart >/dev/null 2>&1 || rescue_fail init-start
    fi
    attempt=0
    while [ "$attempt" -lt 5 ]; do
        if rescue_owned_ready; then
            printf '%s\n' 'RESCUE_READY port=2222'
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 1
    done
    rescue_fail not-ready
}
if [ "${RESCUE_LIBRARY_ONLY:-0}" != 1 ]; then
    [ "$#" = 0 ] || rescue_fail arguments
    rescue_dirs
    rescue_lock
    rescue_reconcile
fi
