param([switch]$SkipTests)

$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') {
    throw 'Run this script on Windows with Rust MSVC and Visual Studio C++ Build Tools installed.'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo not found. Install Rust from https://rustup.rs and reopen PowerShell.'
}

$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    $target = 'x86_64-pc-windows-msvc'
    if (-not $SkipTests) {
        cargo test --workspace --all-targets --locked --target $target
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed; no package was produced.' }
    }
    cargo build --release --locked --target $target
    if ($LASTEXITCODE -ne 0) { throw 'Build failed; no package was produced.' }

    $package = Join-Path $projectRoot 'dist/NekoSportsWorldTool-0.4.1-free-run-win-x64'
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    Copy-Item "target/$target/release/nekosportsworldtool.exe" $package -Force
    Copy-Item 'LICENSE' $package -Force
    Copy-Item 'docs/free-run.md' (Join-Path $package 'FREE-RUN.md') -Force
    $archive = "$package.zip"
    # Package only these files, even if this directory was previously used to run the app.
    $packageFiles = @(
        (Join-Path $package 'nekosportsworldtool.exe'),
        (Join-Path $package 'LICENSE'),
        (Join-Path $package 'FREE-RUN.md')
    )
    Compress-Archive -Path $packageFiles -DestinationPath $archive -Force
    Write-Host "Package: $archive"
} finally {
    Pop-Location
}
