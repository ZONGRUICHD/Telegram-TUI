# Telegram-TUI Windows 安装脚本 (PowerShell)
#
# 用法:
#   irm https://raw.githubusercontent.com/ZONGRUICHD/Telegram-TUI/main/scripts/install.ps1 | iex
#   或
#   .\scripts\install.ps1

$ErrorActionPreference = "Stop"

$REPO_URL = "https://github.com/ZONGRUICHD/Telegram-TUI.git"
$INSTALL_DIR = "$env:USERPROFILE\.cargo\bin"

function Write-Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Ok($msg)   { Write-Host "  ✅ $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "  ⚠️ $msg" -ForegroundColor Yellow }
function Write-Err($msg)  { Write-Host "  ❌ $msg" -ForegroundColor Red }

Write-Host ""
Write-Host "  Telegram-TUI Windows 安装程序" -ForegroundColor Cyan -Bold
Write-Host "  ────────────────────────────" -ForegroundColor Cyan
Write-Host ""

# ── Step 1: 检查 Rust ──────────────────────────────────────────────

Write-Step "检查 Rust 工具链"
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Step "安装 Rust…"
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile "$env:TEMP\rustup-init.exe"
    & "$env:TEMP\rustup-init.exe" -y --quiet
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Err "Rust 安装失败，请手动安装: https://rustup.rs"
        exit 1
    }
}
$rustVer = (rustc --version) -replace 'rustc (\S+).*', '$1'
Write-Ok "Rust $rustVer"

# ── Step 2: 检查 TDLib ─────────────────────────────────────────────

Write-Step "检查 TDLib (tdjson.dll)"

$tdjsonPaths = @(
    "$env:USERPROFILE\tdlib\bin\tdjson.dll",
    "$env:ProgramFiles\tdlib\bin\tdjson.dll",
    "C:\tdlib\bin\tdjson.dll"
)

$tdjsonFound = $false
foreach ($p in $tdjsonPaths) {
    if (Test-Path $p) {
        $dir = Split-Path $p
        $env:PATH = "$dir;$env:PATH"
        Write-Ok "TDLib 已安装: $p"
        $tdjsonFound = $true
        break
    }
}

if (-not $tdjsonFound) {
    Write-Warn "未找到 tdjson.dll"
    Write-Host ""
    Write-Host "  TDLib 需要手动安装:" -ForegroundColor Yellow
    Write-Host "  1. 从 https://github.com/tdlib/td/releases 下载预编译包"
    Write-Host "  2. 解压到 C:\tdlib"
    Write-Host "  3. 将 C:\tdlib\bin 添加到 PATH"
    Write-Host ""
    Write-Host "  或从源码编译 (需要 CMake + Visual Studio Build Tools):"
    Write-Host "    git clone https://github.com/tdlib/td.git"
    Write-Host "    cd td && mkdir build && cd build"
    Write-Host '    cmake .. -DCMAKE_BUILD_TYPE=Release'
    Write-Host "    cmake --build . --config Release"
    Write-Host ""
    exit 1
}

# ── Step 3: 克隆/更新项目 ──────────────────────────────────────────

Write-Step "获取源码"
$projectDir = "$env:TEMP\Telegram-TUI"
if (-not (Test-Path "$projectDir\Cargo.toml")) {
    git clone --depth 1 $REPO_URL $projectDir
} else {
    Push-Location $projectDir
    git pull --ff-only 2>$null
    Pop-Location
}

# ── Step 4: 编译 ───────────────────────────────────────────────────

Write-Step "编译 Telegram-TUI…"
Push-Location $projectDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Err "编译失败"
    Pop-Location
    exit 1
}
Pop-Location

# ── Step 5: 安装 ───────────────────────────────────────────────────

Write-Step "安装到 $INSTALL_DIR"
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
}

Copy-Item "$projectDir\target\release\tg.exe" "$INSTALL_DIR\tg.exe" -Force
Copy-Item "$projectDir\target\release\tgcd.exe" "$INSTALL_DIR\tgcd.exe" -Force
Write-Ok "$INSTALL_DIR\tg.exe"
Write-Ok "$INSTALL_DIR\tgcd.exe"

# 检查 PATH
if ($env:PATH -notlike "*$INSTALL_DIR*") {
    Write-Warn "$INSTALL_DIR 不在 PATH 中"
    $addPath = Read-Host "  是否添加到用户 PATH? (Y/n)"
    if ($addPath -ne "n") {
        [Environment]::SetEnvironmentVariable("Path", "$INSTALL_DIR;" + [Environment]::GetEnvironmentVariable("Path", "User"), "User")
        $env:PATH = "$INSTALL_DIR;$env:PATH"
        Write-Ok "已添加到用户 PATH（重启终端生效）"
    }
}

# ── 完成 ────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "═══════════════════════════════════════════" -ForegroundColor Green
Write-Host "  ✅  Telegram-TUI 安装完成！" -ForegroundColor Green -Bold
Write-Host "═══════════════════════════════════════════" -ForegroundColor Green
Write-Host ""
Write-Host "  下一步:"
Write-Host ""
Write-Host "  1. 初始化配置:" -ForegroundColor Cyan
Write-Host "     tg init"
Write-Host ""
Write-Host "  2. 启动 daemon:" -ForegroundColor Cyan
Write-Host "     tgcd"
Write-Host ""
Write-Host "  3. 登录 (新终端窗口):" -ForegroundColor Cyan
Write-Host "     tg login"
Write-Host ""
Write-Host "  4. 使用:" -ForegroundColor Cyan
Write-Host "     tg chats / tg history <id> / tg send <id> `"text`" / tg tui"
Write-Host ""
Write-Host "  API 凭证: https://my.telegram.org/apps"
Write-Host ""
