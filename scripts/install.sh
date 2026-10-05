#!/bin/sh
# Interactive entry for a downloaded be6500panel release. Run locally, not on WAN.
set -eu
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
command -v ssh >/dev/null 2>&1 || { printf '%s\n' '需要安装 OpenSSH 客户端。' >&2; exit 1; }
printf '%s\n' 'be6500panel 安装' '1. 尚未取得 SSH（仅 RN02 固件 1.0.42 / 1.0.43）' '2. 已有 SSH，直接安装面板' '0. 退出'
printf '%s' '请选择 [1/2/0]: '
IFS= read -r choice || exit 1
case "$choice" in
  1)
    [ -f "$SCRIPT_DIR/enable-ssh.sh" ] || { printf '%s\n' '缺少 enable-ssh.sh，请下载完整安装工具。' >&2; exit 1; }
    sh "$SCRIPT_DIR/enable-ssh.sh"
    printf '%s' '已确认可以通过 SSH 登录，继续安装面板？[y/N]: '
    IFS= read -r confirmed || exit 1
    case "$confirmed" in y|Y) ;; *) exit 0;; esac
    ;;
  2) ;;
  0) exit 0;;
  *) printf '%s\n' '无效选项。' >&2; exit 1;;
esac
[ -f "$SCRIPT_DIR/install-panel.sh" ] || { printf '%s\n' '缺少 install-panel.sh，请下载完整安装工具。' >&2; exit 1; }
if [ "$#" -eq 0 ]; then
    printf '%s' '路由器 IP（默认 192.168.31.1）: '
    IFS= read -r router || exit 1
    router=${router:-192.168.31.1}
    set -- --host "$router"
fi
exec sh "$SCRIPT_DIR/install-panel.sh" "$@"
