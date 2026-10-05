#!/bin/sh
# Authorized existing-SSH installation. No SSH unlock, WAN or Wi-Fi changes.
set -eu
umask 077
host= port=22 user=root release=latest package= public_key= identity=
repo=nkanf-dev/be6500panel
usage() {
    printf '%s\n' 'Usage: install-panel.sh --host ROUTER [--port 22] [--user root] [--release TAG|latest] [--package PATH] [--public-key PATH] [--identity PATH]' 'Local --package requires SHA256SUMS next to the archive. First install reads the panel password from stdin; SSH uses system key/password prompts.'
}
fail() { printf 'Install failed: %s\n' "$1" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
    case "$1" in
        --help|-h) usage; exit 0;;
        --host|--port|--user|--release|--package|--public-key|--identity)
            [ "$#" -ge 2 ] || fail missing-option-value
            case "$1" in
                --host) host=$2;; --port) port=$2;; --user) user=$2;;
                --release) release=$2;; --package) package=$2;;
                --public-key) public_key=$2;; --identity) identity=$2;;
            esac
            shift 2;;
        *) fail unknown-option;;
    esac
done
case "$host" in ''|-*|*[!A-Za-z0-9.-]*) fail invalid-host;; esac
case "$port" in ''|*[!0-9]*) fail invalid-port;; esac
[ "$port" -ge 1 ] && [ "$port" -le 65535 ] || fail invalid-port
[ "$user" = root ] || fail root-required
case "$release" in ''|-*|*[!A-Za-z0-9._-]*) fail invalid-release;; esac
for cmd in curl ssh scp tar awk sed stat gzip cmp mktemp; do command -v "$cmd" >/dev/null || fail "missing command: $cmd"; done
if command -v sha256sum >/dev/null; then
    hash() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null; then
    hash() { shasum -a 256 "$1" | awk '{print $1}'; }
else fail 'missing SHA256 command'; fi
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
for f in router-install.sh router-rescue-setup.sh rescue-bootstrap.sh rescue-ssh.init; do
    [ -f "$script_dir/$f" ] || fail "missing installer tool: $f"
done
work=$(mktemp -d "${TMPDIR:-/tmp}/be6500panel-install.XXXXXXXX")
chmod 700 "$work"
retain=no
cleanup() {
    if [ "$retain" = yes ]; then printf 'Private rollback files retained at: %s\n' "$work" >&2
    else rm -rf "$work"; fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
remote() { ssh -n -p "$port" -o ConnectTimeout=10 "$user@$host" "$@"; }
remote_stdin() { ssh -p "$port" -o ConnectTimeout=10 "$user@$host" "$@"; }
asset=be6500panel-armv7.tar.gz
if [ -z "$package" ]; then
    if [ "$release" = latest ]; then
        latest=$(curl -fsSL --proto '=https' --max-time 30 -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
        case "$latest" in "https://github.com/$repo/releases/tag/"*) release=${latest##*/};; *) fail latest-release-resolution;; esac
        case "$release" in ''|*[!A-Za-z0-9._-]*) fail invalid-release-tag;; esac
    fi
    printf 'Panel release: %s\n' "$release"
    curl -fsSL --proto '=https' --max-time 120 "https://github.com/$repo/releases/download/$release/$asset" -o "$work/$asset"
    curl -fsSL --proto '=https' --max-time 30 "https://github.com/$repo/releases/download/$release/SHA256SUMS" -o "$work/SHA256SUMS"
    package=$work/$asset
    sums=$work/SHA256SUMS
else
    [ -f "$package" ] || fail local-package-not-found
    sums=$(dirname -- "$package")/SHA256SUMS
    [ -f "$sums" ] || fail missing-SHA256SUMS
    printf 'Local release package: %s\n' "$package"
fi
expected=$(awk -v name="$asset" '$2==name||$2=="*"name {n++; h=$1} END {if(n!=1)exit 1; print h}' "$sums") || fail ambiguous-release-checksum
[ "${#expected}" -eq 64 ] || fail invalid-release-checksum
case "$expected" in *[!0-9a-fA-F]*) fail invalid-release-checksum;; esac
actual=$(hash "$package")
[ "$(printf '%s' "$expected" | tr A-F a-f)" = "$actual" ] || fail package-checksum
# Do not extract an archive until all member names and types have been admitted.
tar -tzf "$package" > "$work/members"
awk '
 $0=="be6500-panel"||$0=="bootstrap.sh"||$0=="router-native.lua" {n[$0]++;next}
 $0=="web/"||$0~/^web\/[A-Za-z0-9_.\/-]+$/ {if($0~/(^|\/)\.\.?($|\/)/)exit 1;next}
 {exit 1}
 END {if(n["be6500-panel"]!=1||n["bootstrap.sh"]!=1||n["router-native.lua"]!=1)exit 1}
' "$work/members" || fail package-members
tar -tvzf "$package" | awk 'substr($0,1,1)!="-"&&substr($0,1,1)!="d" {exit 1}' || fail package-links
mkdir "$work/release"
tar -xzf "$package" -C "$work/release"
[ -s "$work/release/web/index.html" ] || fail missing-web-index
printf '%s\n' "$actual" > "$work/panel.sha256"
# Read-only probe must happen before any persistent router write.
remote_stdin 'sh -s' > "$work/probe" <<'PROBE'
set -eu
[ "$(id -u)" = 0 ] || exit 1
model=$(sed -n 's/^HARDWARE=//p' /etc/xiaoqiang_version | tr -d "\"'")
version=$(sed -n 's/^ROM=//p' /etc/xiaoqiang_version | tr -d "\"'")
printf 'model=%s\narchitecture=%s\nfirmware=%s\n' "$model" "$(uname -m)" "$version"
/usr/sbin/ip -4 -o address show dev br-lan scope global | awk '{split($4,a,"/");print "lan="a[1];exit}'
df -Pk /data /tmp | awk 'NR>1 {print "space=" $4 "KiB " $6}'
if [ -e /data/be6500panel/panel.tar.gz ]; then printf 'mode=upgrade\n'; else printf 'mode=fresh\n'; fi
PROBE
cat "$work/probe"
[ "$(sed -n 's/^model=//p' "$work/probe")" = RN02 ] || fail unsupported-model
case "$(sed -n 's/^architecture=//p' "$work/probe")" in armv7l|armv7) ;; *) fail unsupported-architecture;; esac
mode=$(sed -n 's/^mode=//p' "$work/probe")
case "$mode" in fresh|upgrade) ;; *) fail invalid-probe;; esac
if [ -z "$public_key" ]; then
    for candidate in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
        if [ -f "$candidate" ]; then public_key=$candidate; break; fi
    done
    if [ -z "$public_key" ]; then
        command -v ssh-keygen >/dev/null || fail missing-ssh-keygen
        mkdir -p "$HOME/.ssh"; chmod 700 "$HOME/.ssh"
        identity=$HOME/.ssh/be6500panel-rescue
        [ ! -e "$identity" ] || fail rescue-key-public-file-missing
        # RSA works with older router dropbear builds. No fixed router password.
        ssh-keygen -q -t rsa -b 3072 -N '' -f "$identity"
        public_key=$identity.pub
    fi
fi
[ -f "$public_key" ] || fail public-key-not-found
[ -n "$identity" ] || identity=${public_key%.pub}
[ -f "$identity" ] || fail 'matching private key required; use --identity'
# Public-key material is copied; private key stays on host.
case "$(awk 'NR==1{print $1}' "$public_key")" in ssh-rsa|ssh-ed25519|ecdsa-sha2-*) ;; *) fail invalid-public-key;; esac
stage=/tmp/be6500panel-install.$(basename "$work" | sed 's/.*\.//')
remote "umask 077; mkdir '$stage'; chmod 700 '$stage'"
scp -P "$port" "$package" "$user@$host:$stage/panel.tar.gz"
scp -P "$port" "$work/panel.sha256" "$script_dir/router-install.sh" "$script_dir/router-rescue-setup.sh" "$script_dir/rescue-bootstrap.sh" "$script_dir/rescue-ssh.init" "$user@$host:$stage/"
scp -P "$port" "$public_key" "$user@$host:$stage/rescue-authorized-key"
if [ "$mode" = fresh ]; then
    printf 'Choose a panel password: ' >&2
    echo_off=no
    if [ -t 0 ]; then stty -echo; echo_off=yes; fi
    IFS= read -r password || { [ "$echo_off" = no ] || stty echo; fail password-input; }
    [ "$echo_off" = no ] || stty echo
    printf '\n' >&2
    [ -n "$password" ] || fail empty-panel-password
    printf '%s\n' "$password" | remote_stdin "umask 077; cat > '$stage/panel-password'; chmod 600 '$stage/panel-password'"
    unset password
fi
remote "sh '$stage/router-install.sh' prepare '$stage'"
# Host key verification uses normal OpenSSH policy. Never disable it.
ssh -p 2222 -i "$identity" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=10 "$user@$host" "umask 077; printf 'key-login-verified\\n' > '$stage/rescue-verified'; printf 'Rescue key login verified\\n'" || fail rescue-key-login
if [ "$mode" = upgrade ]; then
    mkdir "$work/rollback"
    retain=yes
    for f in panel.tar.gz panel.sha256 bootstrap.sh; do scp -P "$port" "$user@$host:/data/be6500panel/$f" "$work/rollback/$f"; done
    [ "$(hash "$work/rollback/panel.tar.gz")" = "$(cat "$work/rollback/panel.sha256")" ] || fail old-package-checksum
    # Validate old member path before extracting only the manager, never cores.
    [ "$(tar -tzf "$work/rollback/panel.tar.gz" | awk '$0=="be6500-panel"{n++} END{print n+0}')" = 1 ] || fail old-package-manager
    tar -xOzf "$work/rollback/panel.tar.gz" be6500-panel > "$work/rollback/old-manager"
    hash "$work/rollback/old-manager" > "$work/rollback/be6500-panel.sha256"
    rm "$work/rollback/old-manager"
    scp -P "$port" "$work/rollback/be6500-panel.sha256" "$user@$host:$stage/rollback/"
fi
if ! remote "sh '$stage/router-install.sh' apply '$stage'"; then
    printf 'Install did not complete. Rescue SSH remains on port 2222.\n' >&2
    if [ "$mode" = upgrade ]; then
        for f in panel.tar.gz panel.sha256 bootstrap.sh; do scp -P "$port" "$work/rollback/$f" "$user@$host:$stage/rollback/$f" || fail rollback-upload; done
        remote "sh '$stage/router-install.sh' rollback '$stage'" || fail rollback-needs-rescue
    fi
    fail install-not-complete
fi
# apply already verifies authenticated API, current exe SHA and exclusive owner.
printf 'Panel installed. Open http://%s:8787/ . Rescue SSH: port 2222.\n' "$host"
retain=no
