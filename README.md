<p align="center">
  <img src="website/favicon.svg" width="64" height="64" alt="">
</p>

<h1 align="center">tear</h1>

<p align="center">Unpack anything from the right-click menu.<br>A fast, quiet archive tool for Windows, macOS and Linux.</p>

---

Right-click any archive and choose **tear › Extract here**. The files appear immediately, with no splash screen, no pop-ups and nothing left running in the background.

```
Tear  ›  Extract here
         Extract to new folder
         Test archive
```

## Install

**[Download the Windows preview](https://github.com/Xyst69/tear/releases)**, then follow **[INSTALL.md](INSTALL.md)**. In short: unblock and extract the zip, run `Install.cmd`, then right-click any archive and choose **Tear** (on Windows 11, under "Show more options").

## Status

tear is in early development. Nothing has been released yet. [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) is the full engineering plan: architecture, formats, performance targets, shell integration for each OS, and release pipeline.

The Windows x64 preview extracts every format in the table below (creating archives comes later). It adds a per-user **tear** entry to File Explorer's classic right-click menu, which is under "Show more options" on Windows 11. Build a test package with:

```powershell
.\scripts\package-windows.ps1
```

This produces `dist\tear-windows-x64.zip` (and `dist\SHA256SUMS`) with `tear.exe`, `tearw.exe` (the same commands without a console window, used by the menu), `Install.cmd`, `Uninstall.cmd`, sample archives, and a step-by-step guide in `TRY-IT.txt`. See [CONTRIBUTING.md](CONTRIBUTING.md) for the build setup and verification commands.

## Formats

| Format | Extract | Create |
|---|---|---|
| zip | Yes, including AES-256 passwords | Planned |
| 7z | Yes, including AES-256 passwords | Planned |
| rar (v4, v5, multi-part) | Yes, including passwords | No |
| tar, tar.gz, tar.xz, tar.zst, tar.bz2 | Yes | Planned |
| gz, xz, zst, bz2 | Yes | Planned |

## Website

The download site lives in [`website/`](website/) and is served by GitHub Pages from the `gh-pages` branch at <https://xyst69.github.io/tear/>. It reads the latest GitHub release and links each platform to its asset. The asset names are fixed in section 9.3 of the plan.

Publish site changes:

```sh
git subtree push --prefix website origin gh-pages
```

Preview it locally:

```sh
cd website
python -m http.server 8000
```

## Repository layout

```
IMPLEMENTATION_PLAN.md   engineering plan
crates/tear-core/        archive processing library
crates/tear-cli/         tear.exe (console) and tearw.exe (no window)
packaging/windows/       per-user installer, uninstaller, testing guide
scripts/                 Windows build and packaging scripts
tests/corpus/            archive fixture verification
.github/workflows/       Windows CI
website/                 download site (static HTML, CSS, JS)
```
