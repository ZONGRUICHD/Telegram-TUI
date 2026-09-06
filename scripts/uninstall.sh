#!/usr/bin/env bash
# Telegram-TUI 卸载脚本
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.cargo/bin}"
SERVICE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/tg"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/tg"

GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
RESET='\033[0m'

info() { echo -e "${GREEN}==>${RESET} $*"; }
warn() { echo -e "${YELLOW}⚠️${RESET}   $*"; }

echo ""
echo -e "${BOLD}  Telegram-TUI 卸载程序${RESET}"
echo ""

# 停止服务
if systemctl --user is-active tgcd.service &>/dev/null 2>&1; then
    info "停止 tgcd 服务…"
    systemctl --user stop tgcd.service 2>/dev/null || true
fi

if systemctl --user is-enabled tgcd.service &>/dev/null 2>&1; then
    info "禁用 tgcd 服务…"
    systemctl --user disable tgcd.service 2>/dev/null || true
fi

# 删除二进制
for bin in tg tgcd; do
    if [[ -f "${PREFIX}/${bin}" ]]; then
        rm -f "${PREFIX}/${bin}"
        info "已删除 ${PREFIX}/${bin}"
    fi
done

# 删除 systemd 服务
if [[ -f "${SERVICE_DIR}/tgcd.service" ]]; then
    rm -f "${SERVICE_DIR}/tgcd.service"
    systemctl --user daemon-reload 2>/dev/null || true
    info "已删除 systemd 服务"
fi

# 询问是否删除配置和数据
echo ""
read -rp "  是否删除配置和数据？(y/N) " -n 1 answer
echo ""
if [[ "$answer" =~ ^[Yy]$ ]]; then
    rm -rf "$CONFIG_DIR"
    info "已删除配置: $CONFIG_DIR"
    rm -rf "$DATA_DIR"
    info "已删除数据: $DATA_DIR"
else
    info "保留配置和数据"
fi

echo ""
echo -e "${GREEN}✅  卸载完成${RESET}"
echo ""
