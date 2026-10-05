#!/bin/sh
# Shared on-router payload. Only an authorized root SSH connection may call it.
# Host must SHA256-check the release and verify rescue key login before apply.
set -eu
umask 077
fail() { printf 'INSTALL_ERROR %s\n' "$1" >&2; exit 1; }
[ "$#" -eq 2 ] || fail usage
phase=$1
stage=$2
case "$stage" in /tmp/be6500panel-install.*) ;; *) fail stage-path;; esac
suffix=${stage#/tmp/be6500panel-install.}
case "$suffix" in ''|*[!A-Za-z0-9]*) fail stage-path;; esac
[ -d "$stage" ] && [ ! -L "$stage" ] || fail stage-directory
[ "$(stat -c '%u:%a' "$stage")" = '0:700' ] || fail stage-permissions
[ "$(id -u)" = 0 ] || fail root-required
D=/data/be6500panel
R=/tmp/be6500panel
C=/etc/crontabs/root
[ ! -L "$D" ] && [ ! -L "$R" ] || fail unsafe-root
hash() { sha256sum "$1" | awk '{print $1}'; }
instance() { awk '{sub(/^.*\) /, ""); print $20}' "/proc/$1/stat"; }
owner() {
    [ -s "$R/panel.pid" ] || return 1
    pid=$(cat "$R/panel.pid")
    case "$pid" in ''|*[!0-9]*) return 1;; esac
    [ -r "/proc/$pid/stat" ] || return 1
    [ "$(cat "/proc/$pid/comm")" = be6500-panel ] || return 1
    exe=$(readlink "/proc/$pid/exe")
    case "$exe" in "$R/boot-release/be6500-panel"|"$R/boot-release/be6500-panel (deleted)") ;; *) return 1;; esac
    [ "$(hash "/proc/$pid/exe")" = "$expected_exe" ] || return 1
    # The actual exclusive manager, not only a matching process name.
    found=no
    for fd in /proc/"$pid"/fd/*; do
        [ "$(readlink "$fd" 2>/dev/null || :)" != "$D/services/.manager.lock" ] || found=yes
    done
    [ "$found" = yes ] || return 1
    started=$(instance "$pid")
    [ -n "$started" ] || return 1
}
stop_owner() {
    if [ -s "$R/panel.pid" ]; then
        candidate_pid=$(cat "$R/panel.pid")
        case "$candidate_pid" in ''|*[!0-9]*) fail owner-identity;; esac
        if [ ! -e "/proc/$candidate_pid/stat" ]; then rm -f "$R/panel.pid"; fi
    fi
    if [ -s "$R/panel.pid" ]; then
        owner || fail owner-identity
        retained="$pid:$started"
        # TERM is handled by native owner.close(): capture withdrawal and core stop.
        [ "$retained" = "$pid:$(instance "$pid")" ] || fail owner-changed
        kill -TERM "$pid" || fail owner-stop
        n=0
        while [ -e "/proc/$pid/stat" ]; do
            [ "$(instance "$pid")" = "$started" ] || fail pid-reused
            n=$((n+1)); [ "$n" -le 60 ] || fail owner-cleanup-pending
            sleep 1
        done
        rm -f "$R/panel.pid"
    fi
    # Never steal another manager's lock or kill any core PID.
    exec 8>"$D/services/.manager.lock"
    flock -n 8 || fail owner-still-held
    flock -u 8
}
cron_restore() {
    [ -f "$stage/cron-paused" ] || return 0
    cmp -s "$C" "$stage/cron-paused" || fail cron-concurrent-change
    cat "$stage/cron-original" > "$C"
    chmod 600 "$C"
    rm -f "$stage/cron-paused"
    /etc/init.d/cron restart >/dev/null || fail cron-restart
}
cron_pause() {
    [ ! -L "$C" ] || fail cron-symlink
    if [ -f "$C" ]; then cat "$C" > "$stage/cron-original"; else : > "$stage/cron-original"; fi
    # Retain original rows verbatim. Reject unfamiliar commands mentioning us.
    awk '
      index($0,"/data/be6500panel/bootstrap.sh") {
        if ($0 != "* * * * * /data/be6500panel/bootstrap.sh" && $0 != "*/1 * * * * /data/be6500panel/bootstrap.sh") exit 1;
        next
      }
      {print}
    ' "$stage/cron-original" > "$stage/cron-paused" || fail unfamiliar-bootstrap-cron
    cat "$stage/cron-paused" > "$C"
    chmod 600 "$C"
    /etc/init.d/cron restart >/dev/null || fail cron-restart
}
login_health() {
    lan=$(cat "$stage/lan-ip")
    # Lua/cjson already belong to factory RN02; credentials never enter argv.
    lua - "$D/panel-password" > "$stage/login.json" <<'LUA'
local j=require('cjson')
local f=assert(io.open(arg[1],'rb'))
local p=f:read('*a'); f:close()
p=p:gsub('\n$','')
io.write(j.encode({password=p}))
LUA
    curl --noproxy '*' -fsS --max-time 5 -c "$stage/cookies" -H "Origin: http://$lan:8787" -H 'Content-Type: application/json' --data-binary @"$stage/login.json" "http://$lan:8787/api/session/login" > /dev/null || return 1
    curl --noproxy '*' -fsS --max-time 5 -b "$stage/cookies" "http://$lan:8787/api/health" > "$stage/health.json" || return 1
    curl --noproxy '*' -fsS --max-time 5 -b "$stage/cookies" "http://$lan:8787/api/runtime" > "$stage/runtime.json" || return 1
    lua - "$stage/health.json" "$stage/runtime.json" <<'LUA'
local j=require('cjson')
local function load(p) local f=assert(io.open(p)); local v=j.decode(f:read('*a')); f:close(); return v end
local h,r=load(arg[1]),load(arg[2])
assert(h.status=='ok' and h.runtimeEnabled==true and r.enabled==true)
LUA
}
verify() {
    n=0
    while ! login_health 2>/dev/null; do
        n=$((n+1)); [ "$n" -lt 15 ] || fail authenticated-health
        sleep 1
    done
    owner || fail current-owner-identity
    printf 'INSTALL_VERIFIED currentExeSha256=%s ownerInstance=%s:%s\n' "$expected_exe" "$pid" "$started"
    rm -f "$stage/login.json" "$stage/cookies" "$stage/panel-password"
}
case "$phase" in
prepare)
    for cmd in sha256sum tar curl lua flock iptables stat readlink cmp; do command -v "$cmd" >/dev/null || fail missing-router-command; done
    model=$(sed -n 's/^HARDWARE=//p' /etc/xiaoqiang_version | tr -d "\"'")
    [ "$model" = RN02 ] || fail unsupported-model
    case "$(uname -m)" in armv7l|armv7) ;; *) fail unsupported-architecture;; esac
    lan=$(/usr/sbin/ip -4 -o address show dev br-lan scope global | awk '{split($4,a,"/");print a[1];exit}')
    [ -n "$lan" ] || lan=$(/sbin/uci -q get network.lan.ipaddr)
    printf '%s\n' "$lan" | awk -F. 'NF!=4{exit 1} {for(i=1;i<=4;i++)if($i!~/^[0-9]+$/||$i>255)exit 1}' || fail lan-ip
    printf '%s\n' "$lan" > "$stage/lan-ip"
    [ -s "$stage/panel.tar.gz" ] && [ -s "$stage/panel.sha256" ] || fail missing-release
    [ "$(hash "$stage/panel.tar.gz")" = "$(cat "$stage/panel.sha256")" ] || fail package-checksum
    # Complete whitelist, including entry type, before extraction. No credentials,
    # core, private manifests, links, traversal or personal runtime state allowed.
    tar -tzf "$stage/panel.tar.gz" > "$stage/archive-list"
    awk '
      $0=="be6500-panel" || $0=="router-native.lua" || $0=="bootstrap.sh" {n[$0]++;next}
      $0=="web/" || $0 ~ /^web\/[A-Za-z0-9_.\/-]+$/ {if($0 ~ /(^|\/)\.\.?($|\/)/)exit 1;next}
      {exit 1}
      END {if(n["be6500-panel"]!=1||n["router-native.lua"]!=1||n["bootstrap.sh"]!=1)exit 1}
    ' "$stage/archive-list" || fail package-members
    tar -tvzf "$stage/panel.tar.gz" | awk 'substr($0,1,1)!="-"&&substr($0,1,1)!="d" {exit 1}' || fail package-links
    packed=$(wc -c < "$stage/panel.tar.gz")
    # gzip ISIZE includes tar headers; require a conservative extra package copy
    # plus extracted bytes + 1MiB. df /tmp covers tmpfs release reconstruction.
    expanded=$(gzip -dc "$stage/panel.tar.gz" | wc -c)
    data_kb=$(df -Pk /data | awk 'END{print $4}')
    tmp_kb=$(df -Pk /tmp | awk 'END{print $4}')
    [ "$data_kb" -ge $(((packed+1023)/1024+1024)) ] || fail data-space
    [ "$tmp_kb" -ge $(((expanded*2+packed+1023)/1024+1024)) ] || fail runtime-space
    mkdir "$stage/release"
    tar -xzf "$stage/panel.tar.gz" -C "$stage/release"
    [ -s "$stage/release/be6500-panel" ] && [ -s "$stage/release/web/index.html" ] || fail package-content
    hash "$stage/release/be6500-panel" > "$stage/new-exe.sha256"
    if [ -e "$D" ]; then
        [ "$(stat -c '%u:%a' "$D")" = '0:700' ] || fail data-permissions
        for f in panel.tar.gz panel.sha256 panel-password native-command-base.json bootstrap.sh; do
            [ -f "$D/$f" ] && [ ! -L "$D/$f" ] || fail incomplete-existing-install
        done
        printf upgrade > "$stage/mode"
        # No core/history/config copy. Host obtains these three small rollback files.
        mkdir "$stage/rollback"
        # Host checks old archive hash before upload. In-place rename during apply
        # avoids a second persistent package copy.
    else
        printf fresh > "$stage/mode"
        [ -s "$stage/panel-password" ] || fail password-required
        ip_path=$(readlink -f /usr/sbin/ip)
        ipt_path=$(readlink -f "$(command -v iptables)")
        for p in "$ip_path" "$ipt_path"; do
            [ -f "$p" ] && [ -x "$p" ] || fail command-binding
            case "$(stat -c '%a' "$p")" in *[2367][0-7]|*[0-7][2367]) fail writable-command;; esac
        done
        dns=9.9.9.9
        if command -v nslookup >/dev/null && nslookup github.com "$lan" > /dev/null 2>&1; then dns=$lan; fi
        lua - "$ip_path" "$(hash "$ip_path")" "$ipt_path" "$(hash "$ipt_path")" "$dns:53" > "$stage/native-command-base.json" <<'LUA'
local j=require('cjson')
io.write(j.encode({ip={path=arg[1],sha256=arg[2]},iptables={path=arg[3],sha256=arg[4]},dnsBootstrap=arg[5]}))
LUA
    fi
    # Rescue is mandatory and precedes any owner stop or package replacement.
    sh "$stage/router-rescue-setup.sh" --public-key "$stage/rescue-authorized-key" || fail rescue-setup
    printf 'INSTALL_PREPARED mode=%s lan=%s\n' "$(cat "$stage/mode")" "$lan"
    ;;
apply)
    [ -s "$stage/rescue-verified" ] || fail rescue-key-login-required
    [ -s "$stage/mode" ] || fail not-prepared
    mkdir -p "$R"
    chmod 700 "$R"
    exec 9>"$R/.bootstrap.lock"
    flock -n 9 || fail bootstrap-busy
    cron_pause
    trap 'cron_restore' EXIT
    if [ "$(cat "$stage/mode")" = upgrade ]; then
        [ -s "$stage/rollback/be6500-panel.sha256" ] || fail rollback-unavailable
        expected_exe=$(cat "$stage/rollback/be6500-panel.sha256")
        stop_owner
    else
        [ ! -s "$R/panel.pid" ] || fail unexpected-owner
        [ ! -e "$D" ] || fail fresh-data-appeared
        mkdir -m 700 "$D"
        cp "$stage/panel-password" "$D/panel-password"
        cp "$stage/native-command-base.json" "$D/native-command-base.json"
        chmod 600 "$D/panel-password" "$D/native-command-base.json"
    fi
    cp "$stage/panel.tar.gz" "$D/panel.tar.gz.new"
    cp "$stage/panel.sha256" "$D/panel.sha256.new"
    cp "$stage/release/bootstrap.sh" "$D/bootstrap.sh.new"
    chmod 600 "$D/panel.tar.gz.new" "$D/panel.sha256.new"
    chmod 700 "$D/bootstrap.sh.new"
    printf replaced > "$stage/replaced"
    mv "$D/panel.tar.gz.new" "$D/panel.tar.gz"
    mv "$D/panel.sha256.new" "$D/panel.sha256"
    mv "$D/bootstrap.sh.new" "$D/bootstrap.sh"
    flock -u 9
    sh "$D/bootstrap.sh"
    expected_exe=$(cat "$stage/new-exe.sha256")
    verify
    cron_restore
    if [ "$(cat "$stage/mode")" = fresh ]; then
        printf '* * * * * /data/be6500panel/bootstrap.sh\n' >> "$C"
        /etc/init.d/cron restart >/dev/null || fail cron-restart
    fi
    trap - EXIT
    ;;
verify)
    expected_exe=$(cat "$stage/new-exe.sha256")
    verify
    ;;
rollback)
    [ "$(cat "$stage/mode")" = upgrade ] || fail fresh-recovery-retained
    if [ ! -f "$stage/replaced" ]; then cron_restore; exit 0; fi
    mkdir -p "$R"
    chmod 700 "$R"
    exec 9>"$R/.bootstrap.lock"
    flock -n 9 || fail bootstrap-busy
    expected_exe=$(cat "$stage/new-exe.sha256")
    stop_owner
    for f in panel.tar.gz panel.sha256 bootstrap.sh; do
        [ -s "$stage/rollback/$f" ] || fail rollback-unavailable
    done
    [ "$(hash "$stage/rollback/panel.tar.gz")" = "$(cat "$stage/rollback/panel.sha256")" ] || fail rollback-checksum
    for f in panel.tar.gz panel.sha256 bootstrap.sh; do mv "$stage/rollback/$f" "$D/$f"; done
    chmod 600 "$D/panel.tar.gz" "$D/panel.sha256"; chmod 700 "$D/bootstrap.sh"
    flock -u 9
    sh "$D/bootstrap.sh"
    expected_exe=$(cat "$stage/rollback/be6500-panel.sha256")
    verify
    cron_restore
    printf 'INSTALL_ROLLBACK_VERIFIED\n'
    ;;
*) fail unknown-phase;;
esac
