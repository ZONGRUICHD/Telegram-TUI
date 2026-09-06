#!/usr/bin/env bash
#
# Termux (Android) 安装脚本
# Termux 使用 apt 但路径和 Linux 不同
#
set -euo pipefail

PREFIX="${PREFIX:-$PREFIX/bin}"
RUST_LOG="${RUST_LOG:-info}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info() { echo -e "${CYAN}==>${RESET} $*"; }
ok()   { echo -e "${GREEN}✅${RESET}  $*"; }
fail() { echo -e "${RED}❌${RESET}  $*"; exit 1; }

echo ""
echo -e "${CYAN}${BOLD}  Telegram-TUI Termux 安装${RESET}"
echo ""

# 检查 Termux 环境
if [[ ! -d "/data/data/com.termux" ]] && [[ -z "${TERMUX_VERSION:-}" ]]; then
    fail "此脚本仅适用于 Termux (Android)"
fi

# 安装依赖
info "安装依赖…"
pkg update -y
pkg install -y rust cmake git openssl libffi

# 检查 Rust
if ! command -v cargo &>/dev/null; then
    fail "Rust 安装失败"
fi
ok "Rust $(rustc --version | awk '{print $2}')"

# Termux 没有 tdlib 包，需要从源码编译
info "编译 TDLib（首次需要 10-15 分钟）…"

TD_VERSION="1.8.0"
BUILD_DIR="$HOME/.tdlib-build"

if [[ ! -f "$PREFIX/lib/libtdjson.so" ]]; then
    mkdir -p "$BUILD_DIR"
    cd "$BUILD_DIR"

    if [[ ! -d td ]]; then
        git clone --depth 1 --branch "v${TD_VERSION}" https://github.com/tdlib/td.git
    fi

    cd td
    mkdir -p build && cd build

    cmake .. \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$PREFIX" \
        -DTD_ENABLE_JNI=OFF \
        -DCMAKE_C_FLAGS="-I$PREFIX/include" \
        -DCMAKE_CXX_FLAGS="-I$PREFIX/include"

    ncpu=$(nproc 2>/dev/null || echo 4)
    cmake --build . --parallel "$ncpu"
    cmake --install .

    ok "TDLib 编译完成"
else
    ok "TDLib 已安装"
fi

# 编译项目
info "编译 Telegram-TUI…"
cd "$HOME"
if [[ ! -d Telegram-TUI ]]; then
    git clone --depth 1 https://github.com/ZONGRUICHD/Telegram-TUI.git
fi
cd Telegram-TUI
git pull --ff-only 2>/dev/null || true

export LIBRARY_PATH="$PREFIX/lib:$LIBRARY_PATH"
export LD_LIBRARY_PATH="$PREFIX/lib:$LD_LIBRARY_PATH"

cargo build --release

cp -f target/release/tg   "$PREFIX/bin/tg"
cp -f target/release/tgcd "$PREFIX/bin/tgcd"
chmod +x "$PREFIX/bin/tg" "$PREFIX/bin/tgcd"

ok "安装完成"

echo ""
echo -e "  ${BOLD}使用:${RESET}"
echo ""
echo "  tg init      # 初始化配置"
echo "  tgcd &       # 启动 daemon"
echo "  tg login     # 登录"
echo "  tg chats     # 查看聊天"
echo "  tg tui       # TUI 界面"
echo ""
echo "  ${BOLD}注意:${RESET} Termux 没有 systemd，需要手动管理 tgcd 进程"
echo "  建议用 tmux 或 Termux:Boot 保持 tgcd 后台运行"
echo ""
