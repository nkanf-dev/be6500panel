#!/bin/sh
# Adapted from XiaoMi-BE6500-SSH/miwifi-ssh.sh (MIT).
# License: third_party/xiaomi-ssh/LICENSE
# Scope: the owner's Xiaomi BE6500 RN02, firmware 1.0.42 or 1.0.43 only.
# The user reported the four-step unlock working; password login did not work.
# Firmware 1.0.64 and other devices are NOT supported by this script.
# Initial-password evidence: docs/miwifibe6500/static/root-password-review.md
# (external static study of RN02 1.0.43 mkxqimage -I, not current-password proof).
set -eu
set +x
umask 077
LC_ALL=C
export LC_ALL

fail() { printf '%s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "Missing dependency: $1"; }
usage() {
    cat <<'HELP'
Usage: sh scripts/enable-ssh.sh
       sh scripts/enable-ssh.sh --dry-run --firmware 1.0.42|1.0.43
       sh scripts/enable-ssh.sh --calculate-password SN

Interactive mode uses your existing administrator STOK, checks init_info,
asks permission, and sends only the four fixed start_binding requests.
Only your own RN02 on firmware 1.0.42 or 1.0.43 is in scope. Not 1.0.64.
It does not change the administrator or root password, scan other hosts,
or attempt SSH login. A single port-22 SSH-banner probe checks transport.
Dry-run is fully offline: no token, SN, HTTP request or port probe is needed.
--calculate-password displays an INITIAL root-password candidate on request.
That algorithm does not prove the router's CURRENT root password.
Dependencies: Python 3, curl; OpenSSL only for requested password calculation.
HELP
}
supported() { case "$1" in 1.0.42|1.0.43) return 0;; *) return 1;; esac; }
calculate_password() {
    need python3
    need openssl
    # Keep SN/hash input on stdin, not OpenSSL's command line or in a log.
    SN=$(printf '%s' "$1" | python3 -c '
import re, sys
sn = sys.stdin.read().strip("\r\n")
if len(sn) > 64 or not re.fullmatch(r"[A-Za-z0-9]+(?:/[A-Za-z0-9]+)?", sn):
    sys.exit(1)
sys.stdout.write(sn)
') || fail 'Invalid SN: use 1-64 ASCII letters/digits, optionally one slash; no spaces.'
    SALT=6d2df50a-250f-4a30-a5e6-d44fb0960aa0
    DIGEST=$(printf '%s' "${SN}${SALT}" | openssl dgst -md5 -r 2>/dev/null) \
        || fail 'MD5 calculation failed; no password produced.'
    # -r emits the digest FIRST (unlike platform-specific openssl md5 output).
    PASSWORD=$(printf '%s\n' "$DIGEST" | awk '
        NR == 1 && length($1) == 32 && $1 !~ /[^0-9a-fA-F]/ {
            result = tolower(substr($1, 1, 8)); valid = 1
        }
        END { if (valid && NR == 1) print result; else exit 1 }
    ') || fail 'Unexpected MD5 output format; no password produced.'
    printf '%s\n' 'Initial root-password candidate only; the current password may have changed.' >&2
    printf '%s\n' "$PASSWORD"
    unset SN SALT DIGEST PASSWORD
}

case "${1:-}" in
    --help|-h) [ "$#" -eq 1 ] || fail 'Unexpected arguments.'; usage; exit 0;;
    --calculate-password)
        [ "$#" -eq 2 ] || fail 'Usage: --calculate-password SN'
        calculate_password "$2"; exit 0;;
    --dry-run)
        [ "$#" -eq 3 ] && [ "$2" = '--firmware' ] || fail 'Usage: --dry-run --firmware 1.0.42|1.0.43'
        supported "$3" || fail 'Unsupported firmware. Only RN02 1.0.42 and 1.0.43 are in scope; not 1.0.64.'
        printf '%s\n' "OFFLINE dry-run: user-selected RN02 $3; device/version not verified." \
            '[1/4] nvram set ssh_en=1' '[2/4] nvram commit' \
            '[3/4] Set dropbear channel to debug' '[4/4] Start dropbear' \
            'No requests sent. No password computed. No port probe or SSH login.'
        exit 0;;
    '') [ "$#" -eq 0 ] || fail 'Unexpected arguments.';;
    *) usage >&2; exit 1;;
esac

need curl
need python3
printf '%s\n' 'Xiaomi BE6500 RN02 SSH enable request: only firmware 1.0.42 / 1.0.43.' \
    'Use only your own router and an existing administrator STOK.'
printf 'Router IPv4 address [192.168.31.1]: '
IFS= read -r ROUTER_IP || fail 'No router address supplied.'
ROUTER_IP=${ROUTER_IP:-192.168.31.1}
printf '%s' "$ROUTER_IP" | python3 -c '
import ipaddress, sys
try:
    addr = ipaddress.IPv4Address(sys.stdin.read())
    if addr.is_unspecified or addr.is_multicast or addr.is_loopback or str(addr) == "255.255.255.255":
        sys.exit(1)
except ValueError:
    sys.exit(1)
' || fail 'Invalid single-router IPv4 address (no URL, hostname or range).'
printf 'Administrator STOK (32 hex characters; input is not logged): '
# Hide the STOK on an interactive terminal; pipes remain usable for offline tests.
ECHO_HIDDEN=0
restore_echo() { if [ "$ECHO_HIDDEN" -eq 1 ]; then stty "$TTY_STATE" 2>/dev/null || :; fi; }
trap 'restore_echo' 0
trap 'exit 1' HUP INT TERM
if [ -t 0 ] && command -v stty >/dev/null 2>&1; then
    TTY_STATE=$(stty -g 2>/dev/null) || fail 'Cannot read terminal state.'
    stty -echo 2>/dev/null || fail 'Cannot hide token input.'
    ECHO_HIDDEN=1
fi
IFS= read -r STOK || fail 'No STOK supplied.'
restore_echo
ECHO_HIDDEN=0
printf '\n'
case "$STOK" in ''|*[!0-9a-fA-F]*) fail 'Invalid STOK: expected exactly 32 hex characters.';; esac
[ "${#STOK}" -eq 32 ] || fail 'Invalid STOK: expected exactly 32 hex characters.'
BASE="http://${ROUTER_IP}/cgi-bin/luci/;stok=${STOK}/api/xqsystem"

# No redirects, proxy, retries, arbitrary URL or raw response/error output.
# curl -q ignores local curlrc settings that could add logging or redirects.
request() {
    if [ "$1" = GET ]; then
        curl -q --silent --fail --noproxy '*' --proto '=http' \
            --connect-timeout 5 --max-time 15 --max-filesize 65536 \
            --request GET --write-out '\n%{http_code}' "$BASE/init_info" 2>/dev/null
    else
        curl -q --silent --fail --noproxy '*' --proto '=http' \
            --connect-timeout 5 --max-time 15 --max-filesize 65536 \
            --request POST --write-out '\n%{http_code}' \
            --data "$2" "$BASE/start_binding" 2>/dev/null
    fi
}
# Static controller xqsystem.lua: init_info -> getInitInfo returns top-level
# hardware and romversion. Flag9 makes this read noauth-capable: supplying
# STOK here does NOT prove the token is valid. start_binding requires it.
parse_response() {
    python3 -c '
import json, re, sys

def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate")
        result[key] = value
    return result

try:
    raw = sys.stdin.read(65540)
    body, status = raw.rsplit("\n", 1)
    if len(raw) > 65536 or status != "200":
        raise ValueError("http")
    data = json.loads(body, object_pairs_hook=unique)
    if not isinstance(data, dict) or type(data.get("code")) is not int or data["code"] != 0:
        raise ValueError("api")
    if sys.argv[1] == "info":
        hw = data.get("hardware")
        version = data.get("romversion")
        hw = "?" if hw is None or hw == "" else ("RN02" if hw == "RN02" else "unsupported")
        if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
            version = "?"
        print(hw, version)
except (ValueError, TypeError, RecursionError):
    sys.exit(1)
' "$1"
}
INFO='? ?'
if RESPONSE=$(request GET); then
    INFO=$(printf '%s' "$RESPONSE" | parse_response info) || INFO='? ?'
fi
unset RESPONSE
HW=${INFO% *}
VERSION=${INFO#* }
[ "$HW" != unsupported ] || fail 'Unsupported hardware. This script is restricted to RN02.'
if [ "$VERSION" != '?' ]; then
    supported "$VERSION" || fail 'Unsupported reported firmware. Only 1.0.42 / 1.0.43; not 1.0.64.'
fi
if [ "$HW" = '?' ] || [ "$VERSION" = '?' ]; then
    printf '%s\n' 'Device/version could not be fully read. Nothing has been changed.' \
        'Check the router label and firmware in its admin UI before continuing.'
    printf 'Type the confirmed hardware (RN02): '
    IFS= read -r CONFIRMED_HW || fail 'Hardware confirmation missing.'
    [ "$CONFIRMED_HW" = RN02 ] || fail 'Only confirmed RN02 is supported.'
    printf 'Type the confirmed firmware (1.0.42 or 1.0.43): '
    IFS= read -r CONFIRMED_VERSION || fail 'Firmware confirmation missing.'
    supported "$CONFIRMED_VERSION" || fail 'Unsupported confirmed firmware.'
    [ "$VERSION" = '?' ] || [ "$VERSION" = "$CONFIRMED_VERSION" ] || fail 'Firmware confirmation conflicts with the reported version.'
    VERSION=$CONFIRMED_VERSION
    printf '%s\n' "Using user-confirmed RN02 $VERSION; not an independently verified version."
else
    printf '%s\n' "init_info reports RN02 $VERSION (not proof of STOK authentication)."
fi
printf '%s\n' 'The four requests set ssh_en, commit NVRAM, edit dropbear, and start it.' \
    'No password is set. No automatic login is attempted.'
printf 'To authorize these changes on your own router, type ENABLE: '
IFS= read -r AUTHORIZATION || fail 'Authorization missing.'
[ "$AUTHORIZATION" = ENABLE ] || fail 'Cancelled. No enable requests sent.'

step() {
    printf '[%s/4] %s\n' "$1" "$2"
    RESPONSE=$(request POST "$3") || fail "Step $1 HTTP request failed; stopped. Earlier changes may remain."
    printf '%s' "$RESPONSE" | parse_response step \
        || fail "Step $1 rejected or returned invalid JSON; stopped. Earlier changes may remain."
    unset RESPONSE
    printf '%s\n' '  Enable request accepted (code 0); service state is not yet verified.'
}
step 1 'Set ssh_en=1' "uid=1234&key=1234'%0Anvram%20set%20ssh_en%3D1'"
step 2 'Commit NVRAM' "uid=1234&key=1234'%0Anvram%20commit'"
step 3 'Set dropbear channel to debug' "uid=1234&key=1234'%0Ased%20-i%20's%2Fchannel%3D.*%2Fchannel%3D%22debug%22%2Fg'%20%2Fetc%2Finit.d%2Fdropbear'"
step 4 'Start dropbear' "uid=1234&key=1234'%0A%2Fetc%2Finit.d%2Fdropbear%20start'"
unset STOK BASE
printf '%s\n' 'All four enable requests completed. Checking this router on port 22 once.'
# One bounded connection to port 22, read-only SSH identification banner.
# Never send a password or perform authentication. No host-key bypass.
PROBE_STATUS=0
python3 -c '
import socket, sys, time
try:
    with socket.create_connection((sys.argv[1], 22), timeout=3) as conn:
        deadline = time.monotonic() + 3
        banner = b""
        while len(banner) < 1024:
            conn.settimeout(max(0.01, deadline - time.monotonic()))
            chunk = conn.recv(min(256, 1024 - len(banner)))
            if not chunk:
                break
            banner += chunk
            if any(line.startswith(b"SSH-") for line in banner.splitlines()):
                sys.exit(0)
            if time.monotonic() >= deadline:
                break
except OSError:
    pass
sys.exit(1)
' "$ROUTER_IP" 2>/dev/null || PROBE_STATUS=2
if [ "$PROBE_STATUS" -eq 0 ]; then
    printf '%s\n' 'SSH transport detected on port 22. Root login and persistence are not verified.'
else
    printf '%s\n' 'Enable requests completed, but SSH transport was not verified on port 22.' \
        'No retry or password login was attempted. Check the service state manually.'
fi
printf '%s\n' 'The current root password may differ from the factory INITIAL password.'
printf 'Display the SN-derived initial-password candidate? [y/N]: '
IFS= read -r SHOW_PASSWORD || SHOW_PASSWORD=n
case "$SHOW_PASSWORD" in
    y|Y|yes|YES)
        printf 'Router SN (letters/digits, optionally one slash): '
        IFS= read -r SN || fail 'No SN supplied.'
        calculate_password "$SN";;
esac
printf '%s\n' "For a manual login, use your SSH client: ssh root@${ROUTER_IP}" \
    'Use the current password. Do not treat a derived candidate as a verified password.'
exit "$PROBE_STATUS"
