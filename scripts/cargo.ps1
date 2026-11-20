<#
.SYNOPSIS
  Runs Cargo with the native toolchain tear needs on Windows x64.

.DESCRIPTION
  The RAR, xz and zstd codecs compile C and C++ sources. This script provides
  them with the w64devkit GCC toolchain (downloaded once and verified by SHA-256)
  and links tear.exe statically, so the result runs on any Windows 10/11 x64 PC
  without extra DLLs.

.EXAMPLE
  .\scripts\cargo.ps1 build --locked --release
  .\scripts\cargo.ps1 test --locked --workspace
  .\scripts\cargo.ps1 clippy --locked --workspace --all-targets '--' -D warnings

  Quote '--' as shown: PowerShell removes a bare -- before the script sees it.
#>
$ErrorActionPreference = 'Stop'

$version = '2.11.0'
$sha256 = '54C2BB9962E8A30C8270E34C5EFC3D5FEE579F53087FA7D435B95FBBA2AF7B2B'
$tools = Join-Path $env:LOCALAPPDATA 'tear-build-tools'
$root = Join-Path $tools 'w64devkit'
$repo = Split-Path -Parent $PSScriptRoot

if (-not (Test-Path (Join-Path $root 'bin/g++.exe'))) {
    New-Item -ItemType Directory -Force $tools | Out-Null
    $installer = Join-Path $tools "w64devkit-x64-$version.7z.exe"
    if (-not (Test-Path $installer)) {
        Write-Host "Downloading w64devkit $version..."
        $url = "https://github.com/skeeto/w64devkit/releases/download/v$version/w64devkit-x64-$version.7z.exe"
        Invoke-WebRequest -UseBasicParsing $url -OutFile $installer
    }
    if ((Get-FileHash $installer -Algorithm SHA256).Hash -ne $sha256) {
        Remove-Item $installer
        throw "w64devkit download failed verification. Run the script again."
    }
    # The download is a self-extracting 7z archive.
    & $installer -y "-o$tools" | Out-Null
    if (-not (Test-Path (Join-Path $root 'bin/g++.exe'))) { throw 'w64devkit did not extract correctly.' }
}

# w64devkit ships a single static libgcc. Rust's GNU target also asks for libgcc_eh.
$shims = Join-Path $repo 'target/build-shims'
New-Item -ItemType Directory -Force $shims | Out-Null
$gccLib = (& (Join-Path $root 'bin/gcc.exe') -print-libgcc-file-name).Trim()
$eh = Join-Path $shims 'libgcc_eh.a'
if (-not (Test-Path $eh)) { Copy-Item $gccLib $eh }

$bin = Join-Path $root 'bin'
# Environment changes would outlive this script in the caller's session; restore them afterwards.
$names = 'Path', 'CC', 'CXX', 'AR', 'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER', 'RUSTFLAGS'
$saved = @{}
foreach ($n in $names) { $saved[$n] = [Environment]::GetEnvironmentVariable($n, 'Process') }
$env:Path = "$bin;$env:Path"
$env:CC = Join-Path $bin 'gcc.exe'
$env:CXX = Join-Path $bin 'g++.exe'
$env:AR = Join-Path $bin 'ar.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $bin 'g++.exe'
$env:RUSTFLAGS = "-C link-self-contained=no -C link-arg=-ladvapi32 -C link-arg=-static -L native=$shims"

Push-Location $repo
try {
    & cargo @args
    $code = $LASTEXITCODE
} finally {
    Pop-Location
    foreach ($n in $names) { [Environment]::SetEnvironmentVariable($n, $saved[$n], 'Process') }
}
exit $code
