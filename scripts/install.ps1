# Run this file from a checkout. See docs/BUILD.md for native dependency setup.
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs/Telegram-TUI'),
    [string]$Toolchain = ''
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$nativeLibrary = Join-Path $projectRoot 'native/tdjson.dll'
if (-not (Test-Path -LiteralPath $nativeLibrary)) {
    throw 'Build the pinned TDLib into native/ first; see docs/BUILD.md or use a GitHub Actions package.'
}
Get-Command cargo -ErrorAction Stop | Out-Null
Push-Location $projectRoot
try {
    $cargoArgs = @('build', '--workspace', '--release', '--locked')
    if ($Toolchain) { $cargoArgs = @("+$Toolchain") + $cargoArgs }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    Copy-Item native/*.dll target/release/
    $metadata = & ./target/release/tgcd.exe --check-library
    if ($LASTEXITCODE -ne 0) { throw 'TDLib could not load' }
    $library = $metadata | ConvertFrom-Json
    if ($library.commit -ne $library.expected_commit) { throw 'Wrong TDLib revision' }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item target/release/tg.exe,target/release/tgcd.exe $InstallDir
    Copy-Item native/* $InstallDir
    Copy-Item README.md,LICENSE,TDLIB_COMMIT,THIRD_PARTY_NOTICES.md $InstallDir
    Copy-Item docs -Destination $InstallDir -Recurse -Force
    Write-Host "Installed to $InstallDir"
    Write-Host 'Add this directory to PATH, then run tg doctor and tg login.'
} finally {
    Pop-Location
}
