# Contributing to tear

## Build setup (Windows x64)

Install Rust with rustup. Running Cargo in this checkout selects the version in `rust-toolchain.toml`.

The RAR, xz and zstd codecs compile C and C++ code. `scripts\cargo.ps1` wraps Cargo with the toolchain they need: on first use it downloads [w64devkit](https://github.com/skeeto/w64devkit) 2.11.0 to `%LOCALAPPDATA%\tear-build-tools`, verifies its SHA-256, and links tear statically. Use it in place of `cargo`:

```powershell
.\scripts\cargo.ps1 build --locked --release --bins
```

PowerShell removes a bare `--` before the script sees it, so quote it: `'--'`.

## Verify a change

```powershell
cargo fmt --all --check
.\scripts\cargo.ps1 clippy --locked --workspace --all-targets '--' -D warnings
.\scripts\cargo.ps1 test --locked --workspace
.\scripts\cargo.ps1 build --locked --release --bins
python tests\corpus\verify_windows.py target\release\tear.exe --sevenzip "C:\Program Files\7-Zip\7z.exe"
cargo install cargo-deny --version 0.22.0 --locked
cargo deny check
(Get-Item target\release\tear.exe).Length
```

The corpus script builds about 70 archives with known content (zip, 7z, tar and the compressed stream formats, including encrypted, damaged and Mark-of-the-Web cases) and checks every byte tear extracts. The `--sevenzip` cases need 7-Zip installed.

## Package for testing

```powershell
.\scripts\package-windows.ps1
```

This writes `dist\tear-windows-x64.zip` and `dist\SHA256SUMS`, the assets attached to each GitHub release. `packaging\windows\TRY-IT.txt` inside it explains installation and what to try.

## Rules

The stripped executable must stay within 15,000,000 bytes. Include a one-line justification for each new dependency in the pull request, and report measured results for performance changes. The engineering plan defines the phase gates and release asset names.
