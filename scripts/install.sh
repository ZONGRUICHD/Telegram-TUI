#!/usr/bin/env bash
#
# Telegram-TUI 一键安装脚本
# 支持 Arch / Debian|Ubuntu / macOS
#
# 用法:
#   curl -fsSL https://raw.githubusercontent.com/ZONGRUICHD/Telegram-TUI/main/scripts/install.sh | bash
#   或
#   ./scripts/install.sh
#
# 环境变量:
#   PREFIX          安装目录 (默认 ~/.cargo/bin)
#   BUILD_TDLIB=1   强制从源码编译 TDLib
#   SKIP_SYSTEMD=1  跳过 systemd 服务安装
#
set -euo pipefail

# 保存脚本启动时的源码目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ── 颜色 ────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info()  { echo -e "${CYAN}==>${RESET} $*"; }
ok()    { echo -e "${GREEN}✅${RESET}  $*"; }
warn()  { echo -e "${YELLOW}⚠️${RESET}   $*"; }
fail()  { echo -e "${RED}❌${RESET}  $*"; exit 1; }

# ── 配置 ────────────────────────────────────────────────────────────

PREFIX="${PREFIX:-$HOME/.cargo/bin}"
REPO_URL="https://github.com/ZONGRUICHD/Telegram-TUI.git"
TD_VERSION="1.8.0"

# ── OS 检测 ──────────────────────────────────────────────────────────

detect_os() {
    if [[ -n "${TERMUX_VERSION:-}" ]] || [[ -d "/data/data/com.termux" ]]; then
        OS="termux"
        PKG_MGR="pkg"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        OS="macos"
        PKG_MGR="brew"
    elif command -v pacman &>/dev/null; then
        OS="arch"
        PKG_MGR="pacman"
    elif command -v apt &>/dev/null; then
        OS="debian"
        PKG_MGR="apt"
    elif command -v dnf &>/dev/null; then
        OS="fedora"
        PKG_MGR="dnf"
    else
        OS="unknown"
        PKG_MGR=""
    fi
    info "操作系统: ${BOLD}${OS}${RESET}"
}

# ── 依赖检查/安装 ────────────────────────────────────────────────────

check_cmd() {
    command -v "$1" &>/dev/null
}

install_pkg() {
    local pkg="$1"
    info "安装 ${pkg}…"
    case "$PKG_MGR" in
        pacman) sudo pacman -S --needed --noconfirm "$pkg" ;;
        apt)    sudo apt-get update -qq && sudo apt-get install -y -qq "$pkg" ;;
        brew)   brew install "$pkg" ;;
        *)      warn "请手动安装 $pkg"; return 1 ;;
    esac
}

# ── Step 1: Rust 工具链 ─────────────────────────────────────────────

install_rust() {
    if check_cmd rustc && check_cmd cargo; then
        ok "Rust $(rustc --version | awk '{print $2}') 已安装"
        return
    fi

    info "安装 Rust 工具链…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet

    # source cargo env（兼容 curl|bash 场景）
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi

    # 确保 cargo 在 PATH 中
    export PATH="$HOME/.cargo/bin:$PATH"

    if ! check_cmd cargo; then
        fail "Rust 安装失败，请手动安装: https://rustup.rs"
    fi

    ok "Rust $(rustc --version | awk '{print $2}') 已安装"
}

# ── Step 2: 系统依赖 ────────────────────────────────────────────────

install_system_deps() {
    info "检查系统依赖…"

    local pkgs=()

    case "$OS" in
        arch)
            check_cmd cmake      || pkgs+=(cmake)
            check_cmd git        || pkgs+=(git)
            check_cmd gcc        || pkgs+=(gcc)
            check_cmd pkg-config || pkgs+=(pkgconf)
            ;;
        debian)
            check_cmd cmake      || pkgs+=(cmake)
            check_cmd git        || pkgs+=(git)
            check_cmd gcc        || pkgs+=(build-essential)
            check_cmd pkg-config || pkgs+=(pkg-config)
            ;;
        fedora)
            check_cmd cmake      || pkgs+=(cmake)
            check_cmd git        || pkgs+=(git)
            check_cmd gcc        || pkgs+=(gcc)
            check_cmd pkg-config || pkgs+=(pkgconf)
            ;;
        termux)
            check_cmd cmake      || pkgs+=(cmake)
            check_cmd git        || pkgs+=(git)
            check_cmd pkg-config || pkgs+=(pkg-config)
            ;;
        macos)
            check_cmd cmake || pkgs+=(cmake)
            check_cmd git   || pkgs+=(git)
            ;;
    esac

    if [[ ${#pkgs[@]} -gt 0 ]]; then
        for p in "${pkgs[@]}"; do
            install_pkg "$p" || warn "跳过 $p"
        done
    fi

    ok "系统依赖就绪"
}

# ── Step 3: TDLib / libtdjson ────────────────────────────────────────

find_libtdjson() {
    # pkg-config
    if check_cmd pkg-config && pkg-config --exists tdjson 2>/dev/null; then
        ok "libtdjson 已安装 (pkg-config)"
        return 0
    fi

    # 搜索常见路径
    local search_paths=(
        "/usr/lib/libtdjson.so"
        "/usr/local/lib/libtdjson.so"
        "/usr/lib/x86_64-linux-gnu/libtdjson.so"
        "/opt/homebrew/lib/libtdjson.dylib"
        "/usr/local/lib/libtdjson.dylib"
    )

    for p in "${search_paths[@]}"; do
        if [[ -f "$p" ]]; then
            ok "libtdjson 已安装: $p"
            local dir
            dir=$(dirname "$p")
            export LIBRARY_PATH="${dir}:${LIBRARY_PATH:-}"
            export LD_LIBRARY_PATH="${dir}:${LD_LIBRARY_PATH:-}"
            return 0
        fi
    done

    return 1
}

install_tdlib_system() {
    info "尝试通过包管理器安装 TDLib…"

    case "$OS" in
        arch)
            # Arch 官方仓库的 tdlib 是 treedec（图论库），不是 Telegram TDLib
            # 需要从 AUR 安装 telegram-tdlib（需要 yay/paru）或从源码编译
            if check_cmd yay; then
                yay -S --needed --noconfirm telegram-tdlib && return 0
            elif check_cmd paru; then
                paru -S --needed --noconfirm telegram-tdlib && return 0
            fi
            warn "Arch 需要 AUR 助手 (yay/paru) 或从源码编译 TDLib"
            return 1
            ;;
        debian|termux)
            if apt-cache show libtd-dev &>/dev/null 2>&1; then
                install_pkg "libtd-dev" && return 0
            fi
            ;;
        fedora)
            if dnf list tdlib-devel &>/dev/null 2>&1; then
                install_pkg "tdlib-devel" && return 0
            fi
            ;;
        macos)
            brew install tdlib && return 0
            ;;
    esac

    return 1
}

build_tdlib_from_source() {
    info "从源码编译 TDLib ${TD_VERSION}…"

    local sudo_needed=""
    if [[ ! -w "/usr/local" ]]; then
        sudo_needed="sudo"
    fi

    # 安装编译依赖
    case "$OS" in
        arch)
            sudo pacman -S --needed --noconfirm cmake gperf zlib openssl ;;
        debian)
            sudo apt-get update -qq
            sudo apt-get install -y -qq cmake gperf zlib1g-dev libssl-dev ;;
        fedora)
            sudo dnf install -y cmake gperf zlib-devel openssl-devel ;;
        termux)
            pkg install -y cmake gperf openssl libffi ;;
        macos)
            brew install cmake gperf openssl ;;
    esac

    local build_dir="/tmp/tdlib-build-${TD_VERSION}"
    mkdir -p "$build_dir"

    # 在子 shell 中编译，不改变当前工作目录
    (
        cd "$build_dir"

        if [[ ! -d td ]]; then
            git clone --depth 1 --branch "v${TD_VERSION}" https://github.com/tdlib/td.git
        fi

        cd td
        mkdir -p build && cd build

        cmake .. \
            -DCMAKE_BUILD_TYPE=Release \
            -DCMAKE_INSTALL_PREFIX=/usr/local \
            -DTD_ENABLE_JNI=OFF

        local ncpu
        ncpu=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
        cmake --build . --parallel "$ncpu"

        ${sudo_needed} cmake --install .
    )

    # 刷新 linker 缓存
    if command -v ldconfig &>/dev/null; then
        ${sudo_needed} ldconfig
    fi

    ok "TDLib 编译安装完成"
}

setup_tdlib() {
    if [[ "${BUILD_TDLIB:-}" == "1" ]]; then
        build_tdlib_from_source
        return
    fi

    if find_libtdjson; then
        return
    fi

    warn "未找到 libtdjson"

    # 尝试包管理器
    if install_tdlib_system; then
        if find_libtdjson; then
            return
        fi
    fi

    # 回退到源码编译
    warn "包管理器未提供 TDLib，将从源码编译（约 5-10 分钟）"
    build_tdlib_from_source
}

# ── Step 4: 编译项目 ────────────────────────────────────────────────

build_project() {
    info "编译 Telegram-TUI…"

    # 确定项目目录
    local project_dir=""
    if [[ -f "${SCRIPT_DIR}/Cargo.toml" ]] && grep -q "tg-tdjson" "${SCRIPT_DIR}/Cargo.toml" 2>/dev/null; then
        project_dir="$SCRIPT_DIR"
    elif [[ -f "Cargo.toml" ]] && grep -q "tg-tdjson" Cargo.toml 2>/dev/null; then
        project_dir="$(pwd)"
    fi

    if [[ -z "$project_dir" ]]; then
        project_dir="/tmp/Telegram-TUI"
        if [[ ! -d "$project_dir" ]]; then
            info "克隆仓库…"
            git clone --depth 1 "$REPO_URL" "$project_dir"
        else
            cd "$project_dir"
            git pull --ff-only 2>/dev/null || true
        fi
    fi

    info "项目目录: ${project_dir}"

    # 在子 shell 中编译
    (
        cd "$project_dir"
        cargo build --release 2>&1 | tail -5
    )

    # 保存项目路径供后续使用
    PROJECT_DIR="$project_dir"
    ok "编译完成"
}

# ── Step 5: 安装二进制 ──────────────────────────────────────────────

install_binaries() {
    info "安装到 ${PREFIX}…"
    mkdir -p "$PREFIX"

    local src_dir="${PROJECT_DIR:-$SCRIPT_DIR}/target/release"
    local bins=("tg" "tgcd")

    for bin in "${bins[@]}"; do
        if [[ -f "${src_dir}/${bin}" ]]; then
            cp -f "${src_dir}/${bin}" "${PREFIX}/${bin}"
            chmod +x "${PREFIX}/${bin}"
            ok "${PREFIX}/${bin}"
        else
            warn "未找到 ${src_dir}/${bin}"
        fi
    done

    # 确保 PREFIX 在 PATH 中
    if [[ ":$PATH:" == *":${PREFIX}:"* ]]; then
        ok "${PREFIX} 已在 PATH 中"
        return
    fi

    warn "${PREFIX} 不在 PATH 中"

    # 自动添加到 shell 配置
    local shell_rc=""
    if [[ "$SHELL" == */zsh ]]; then
        shell_rc="$HOME/.zshrc"
    elif [[ "$SHELL" == */bash ]]; then
        shell_rc="$HOME/.bashrc"
    fi

    if [[ -n "$shell_rc" ]] && [[ -f "$shell_rc" ]]; then
        if ! grep -q "export PATH=.*${PREFIX}" "$shell_rc" 2>/dev/null; then
            {
                echo ""
                echo "# Telegram-TUI"
                echo "export PATH=\"${PREFIX}:\$PATH\""
            } >> "$shell_rc"
            info "已添加到 ${shell_rc}"
            echo ""
            echo "  运行以下命令使 PATH 生效:"
            echo "    source ${shell_rc}"
            echo ""
        fi
    else
        echo ""
        echo "  手动添加 PATH:"
        echo "    export PATH=\"${PREFIX}:\$PATH\""
        echo ""
    fi
}

# ── Step 6: Systemd 服务 ────────────────────────────────────────────

setup_systemd() {
    if [[ "${SKIP_SYSTEMD:-}" == "1" ]]; then
        info "跳过 systemd 安装"
        return
    fi

    if ! check_cmd systemctl; then
        warn "systemctl 未找到，跳过 systemd 服务安装"
        return
    fi

    local service_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    mkdir -p "$service_dir"

    cat > "${service_dir}/tgcd.service" << EOF
[Unit]
Description=Telegram CLI Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${PREFIX}/tgcd
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload 2>/dev/null || true
    systemctl --user enable tgcd.service 2>/dev/null || true

    ok "systemd 服务已安装: ${service_dir}/tgcd.service"
}

# ── Step 7: 完成 ────────────────────────────────────────────────────

print_summary() {
    echo ""
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════${RESET}"
    echo -e "${GREEN}${BOLD}  ✅  Telegram-TUI 安装完成！${RESET}"
    echo -e "${GREEN}${BOLD}═══════════════════════════════════════════${RESET}"
    echo ""
    echo -e "  ${BOLD}二进制:${RESET}  ${PREFIX}/tg  ${PREFIX}/tgcd"
    echo ""
    echo -e "  ${BOLD}下一步:${RESET}"
    echo ""
    echo -e "  ${CYAN}1.${RESET} 初始化配置（需要 api_id + api_hash）:"
    echo -e "     ${BOLD}tg init${RESET}"
    echo ""
    echo -e "  ${CYAN}2.${RESET} 启动 daemon:"
    echo -e "     ${BOLD}systemctl --user start tgcd${RESET}"
    echo -e "     或: ${BOLD}tgcd &${RESET}"
    echo ""
    echo -e "  ${CYAN}3.${RESET} 登录 Telegram:"
    echo -e "     ${BOLD}tg login${RESET}"
    echo ""
    echo -e "  ${CYAN}4.${RESET} 使用:"
    echo -e "     ${BOLD}tg chats${RESET}            列出聊天"
    echo -e "     ${BOLD}tg history <id>${RESET}      查看消息"
    echo -e "     ${BOLD}tg send <id> \"你好\"${RESET}  发消息"
    echo -e "     ${BOLD}tg tui${RESET}              终端界面"
    echo ""
    echo -e "  API 凭证: https://my.telegram.org/apps"
    echo ""
}

# ── 主流程 ──────────────────────────────────────────────────────────

main() {
    echo ""
    echo -e "${CYAN}${BOLD}  Telegram-TUI 安装程序${RESET}"
    echo -e "${CYAN}  ─────────────────────${RESET}"
    echo ""

    detect_os
    install_rust
    install_system_deps
    setup_tdlib
    build_project
    install_binaries
    setup_systemd
    print_summary
}

main "$@"
