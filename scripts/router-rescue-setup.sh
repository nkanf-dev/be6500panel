#!/bin/sh
# Router-side API: sh router-rescue-setup.sh --public-key /tmp/stage/key.pub
# No passwords, private-key input, arbitrary command input, or network download.
set -eu
umask 077
fail() { printf 'RESCUE_ERROR %s\n' "$1" >&2; exit 1; }
[ "$#" = 2 ] && [ "$1" = --public-key ] || fail arguments
KEY=$2
case "$KEY" in /*) ;; *) fail public-key-path;; esac
[ -f "$KEY" ] && [ ! -L "$KEY" ] && [ -s "$KEY" ] || fail public-key-file
[ "$(wc -c < "$KEY" | tr -d ' ')" -le 16384 ] || fail public-key-format
# Exactly one unqualified public key; authorized_keys options are not input.
awk '
  NF {
    count++
    if ($1 !~ /^(ssh-rsa|ssh-ed25519|ecdsa-sha2-nistp(256|384|521))$/ || NF<2 ||
        ($2 !~ /^[A-Za-z0-9+\/]+==?$/ && $2 !~ /^[A-Za-z0-9+\/]+$/)) bad=1
  }
  END {exit (count!=1 || bad)}' "$KEY" || fail public-key-format
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
for file in rescue-bootstrap.sh rescue-ssh.init; do
    [ -f "$SCRIPT_DIR/$file" ] && [ ! -L "$SCRIPT_DIR/$file" ] || fail missing-assets
done
RESCUE_LIBRARY_ONLY=1
. "$SCRIPT_DIR/rescue-bootstrap.sh"
rescue_dirs
rescue_lock
for file in "$DATA/bin/dropbear" "$DATA/bin/dropbear.sha256" \
    "$DATA/bin/dropbearkey" "$DATA/dropbear_rsa_host_key" "$DATA/authorized_keys" \
    "$DATA/rescue-bootstrap.sh" "$DATA/rescue-bootstrap.sh.sha256" \
    "$DATA/rescue-ssh.init" "$DATA/rescue-ssh.init.sha256"; do
    [ ! -L "$file" ] || rescue_fail unsafe-path
    [ ! -e "$file" ] || [ -f "$file" ] || rescue_fail unsafe-path
done
# RN02 factory multicall dropbear is /usr/sbin/dropbear. Its key utility is
# /usr/bin/dropbearkey -> ../sbin/dropbear. Retain the working saved binary.
if [ -e "$DATA/bin/dropbear" ]; then
    rescue_digest "$DATA/bin/dropbear" || rescue_fail binary-integrity
else
    [ -f /usr/sbin/dropbear ] && [ -x /usr/sbin/dropbear ] && [ ! -L /usr/sbin/dropbear ] || rescue_fail factory-binary
    cp /usr/sbin/dropbear "$DATA/bin/dropbear"
    sha256sum "$DATA/bin/dropbear" | awk '{print $1}' > "$DATA/bin/dropbear.sha256"
fi
chmod 700 "$DATA/bin/dropbear"
if [ ! -s "$DATA/dropbear_rsa_host_key" ]; then
    if rescue_regular "$NATIVE/dropbear_rsa_host_key" && [ -s "$NATIVE/dropbear_rsa_host_key" ]; then
        cp "$NATIVE/dropbear_rsa_host_key" "$DATA/dropbear_rsa_host_key"
    else
        # Invoking the copied multicall binary as dropbearkey creates fresh keys.
        cp "$DATA/bin/dropbear" "$DATA/bin/dropbearkey"
        chmod 700 "$DATA/bin/dropbearkey"
        "$DATA/bin/dropbearkey" -t rsa -s 2048 -f "$DATA/dropbear_rsa_host_key" >/dev/null 2>&1 || rescue_fail host-key-generation
        rm -f "$DATA/bin/dropbearkey"
    fi
fi
chmod 600 "$DATA/dropbear_rsa_host_key"
# Merge existing persistent and factory keys before adding the supplied row.
touch "$DATA/authorized_keys"
rescue_restore_keys
kind=$(awk 'NF {print $1}' "$KEY")
blob=$(awk 'NF {print $2}' "$KEY")
printf '%s\n' "$blob" | base64 -d >/dev/null 2>&1 || rescue_fail public-key-format
if ! awk -v k="$kind" -v b="$blob" '$1==k && $2==b {f=1} END {exit !f}' "$DATA/authorized_keys"; then
    # Ensure the existing last key has a newline; never overwrite old rows.
    [ ! -s "$DATA/authorized_keys" ] || printf '\n' >> "$DATA/authorized_keys"
    printf '%s %s\n' "$kind" "$blob" >> "$DATA/authorized_keys"
fi
chmod 600 "$DATA/authorized_keys"
for file in rescue-bootstrap.sh rescue-ssh.init; do
    # Staging can be /data/ssh on a manual repair, so avoid cp same-file.
    if ! cmp -s "$SCRIPT_DIR/$file" "$DATA/$file"; then
        cp "$SCRIPT_DIR/$file" "$DATA/$file"
    fi
    chmod 700 "$DATA/$file"
    sha256sum "$DATA/$file" | awk '{print $1}' > "$DATA/$file.sha256"
done
# Replace only this exact owned command/marker; preserve all panel and factory
# cron tasks. crontab installs through the firmware's existing persistent path.
if ! crontab -l > "$RUN/cron.old" 2>/dev/null; then
    [ ! -s /etc/crontabs/root ] || rescue_fail cron-read
    : > "$RUN/cron.old"
fi
awk '
  /# be6500-rescue-bootstrap$/ {next}
  $0=="* * * * * /bin/sh /data/ssh/rescue-bootstrap.sh >/dev/null 2>&1" {next}
  {print}' "$RUN/cron.old" > "$RUN/cron.next"
printf '%s\n' '* * * * * /bin/sh /data/ssh/rescue-bootstrap.sh >/dev/null 2>&1 # be6500-rescue-bootstrap' >> "$RUN/cron.next"
crontab "$RUN/cron.next" >/dev/null 2>&1 || rescue_fail cron-install
rm -f "$RUN/cron.old" "$RUN/cron.next"
rescue_reconcile
