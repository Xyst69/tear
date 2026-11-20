# tear: Implementation Plan

> Paste this whole document into the implementing model as its brief. Everything below is written for the engineer or agent building tear.

---

## 0. Your role and how to work

You are the lead engineer building **tear**, a cross-platform archive utility for Windows, macOS and Linux. It does the job 7-Zip does from the file manager's right-click menu, but it is faster, quieter and cleaner.

Work rules:

1. Build in the phase order in section 10. Do not start a phase until the previous phase meets its exit criteria.
2. At the end of each phase, report what you built, the commands you ran, test and benchmark results with real numbers, and anything you deferred. Never claim a result you did not measure.
3. Keep the decisions in this document. If one turns out to be wrong, stop, say why, propose a replacement, and wait for approval before changing course.
4. All code is production quality: no placeholder logic, no `todo!()` or `unwrap()` on user-controlled data, no commented-out code, and no dependencies added without a one-line justification in the PR description.
5. The repository is `github.com/Xyst69/tear`. The download website already exists in `website/` and depends on the release asset names in section 9.3. Do not rename them.

---

## 1. Product definition

### 1.1 One-sentence pitch

Right-click any archive and pick **tear › Extract here**. The files appear immediately, with no splash screen, no pop-ups and nothing left running in the background.

### 1.2 Goals

| Goal | Measurable target |
|---|---|
| Fast | Matches or beats 7-Zip 24.x/25.x on every benchmark in section 7, and is at least 1.5× faster on the many-small-files case on Windows with Defender enabled |
| Feels instant | From the click in the context menu to the first byte written: under 150 ms warm and under 400 ms cold |
| Quiet | No window at all for jobs that finish within 400 ms. No tray icon, no notifications, no sounds, no update nags, no telemetry, no ads |
| Clean | Every menu item has exactly one job, and no menu has more than 6 items (section 5) |
| Native everywhere | One top-level menu entry in Windows 11 Explorer (modern and classic menus), macOS Finder, and the Nautilus, Dolphin, Nemo, Thunar and Caja file managers on Linux |
| Safe | Never writes outside the destination folder, whatever the archive contains (section 6) |

### 1.3 Non-goals for v1

- Creating RAR archives. This is legally impossible with the UnRAR license, so tear only extracts RAR.
- A dual-pane file manager like 7-Zip's File Manager. tear has a single-pane archive browser only.
- Plugins, themes or skins.
- Mobile platforms.

### 1.4 Format matrix (v1)

| Format | Extract | Create | Notes |
|---|---|---|---|
| zip, zipx | Yes | Yes | Store, Deflate, Deflate64, BZip2, LZMA, Zstd, XZ; ZipCrypto (read only) and AES-256 |
| 7z | Yes | Yes | LZMA, LZMA2, PPMd, BZip2, Deflate, Copy; BCJ/BCJ2/ARM64 filters; AES-256; solid and non-solid |
| rar (v4, v5) | Yes | No | Through UnRAR; multi-volume and encrypted archives supported |
| tar | Yes | Yes | ustar, pax, GNU long names, sparse files (read) |
| tar.gz / tgz, tar.xz / txz, tar.zst / tzst, tar.bz2 / tbz2 | Yes | Yes | Streamed through a single pass |
| gz, xz, zst, bz2 (single file) | Yes | Yes | |
| iso, cab | v1.1 | No | Out of scope for v1.0 |

The website lists exactly the v1 rows. Keep the two in sync.

---

## 2. Architecture

```
┌──────────────────────── file manager ────────────────────────┐
│ Explorer (COM IExplorerCommand) │ Finder (Finder Sync ext.)  │
│ Nautilus/Caja (python ext.)     │ Dolphin/Nemo/Thunar (files)│
└───────────────┬──────────────────────────────────────────────┘
                │ spawns:  tear x --smart "C:\a\b.zip"
                ▼
┌──────────────────────── tear (one binary) ───────────────────┐
│ cli (clap) ─► job runner ─► progress UI (lazy, after 400 ms)│
│                   │                                          │
│                   ▼                                          │
│              tear-core (library, no UI, no global state)     │
│   format readers/writers · extraction planner · safe paths  │
│   parallel scheduler · writer pool · checksums              │
└──────────────────────────────────────────────────────────────┘
```

Key decisions:

1. **Language: Rust (stable, MSRV pinned in `rust-toolchain.toml`).** It gives memory safety in parsers that read hostile input, C-level speed, and a single static binary per platform.
2. **One binary, `tear`,** serves as the CLI, the job runner and the GUI. Shell integrations stay thin: each one only spawns `tear` with arguments and never links the core. This keeps Explorer and Finder safe from crashes and makes the menu appear instantly.
3. **GUI toolkit: Slint** with its native-feeling styles (`fluent` on Windows, `cupertino` on macOS, `fluent`/`material` on Linux). It renders natively without a webview, starts cold in under 100 ms, and keeps the binary small. Show "Made with Slint" in the About window to meet the royalty-free license. Fallback if Slint blocks progress: `egui` for dialogs only. Electron and Tauri are rejected because of cold-start time and size.
4. **The process exits when the job is done.** There is no resident service and no daemon.

### 2.1 Repository layout

```
tear/
├─ Cargo.toml                    # workspace
├─ rust-toolchain.toml
├─ crates/
│  ├─ tear-core/                 # formats, planner, safety, scheduler (lib)
│  ├─ tear-cli/                  # `tear` binary: CLI + job runner + GUI entry
│  ├─ tear-ui/                   # Slint UI (.slint files + Rust glue)
│  ├─ tear-shell-windows/        # cdylib: IExplorerCommand COM server
│  └─ tear-bench/                # benchmark harness + corpus generator
├─ integrations/
│  ├─ windows/                   # sparse MSIX manifest, WiX source, registry
│  ├─ macos/                     # Xcode project: app wrapper + Finder Sync ext
│  └─ linux/                     # nautilus/caja .py, dolphin .desktop,
│                                # nemo .nemo_action, thunar uca snippet,
│                                # tear.desktop, MIME xml
├─ fuzz/                         # cargo-fuzz targets per format
├─ tests/corpus/                 # small fixture archives (+ generator script)
├─ website/                      # download site (already built)
└─ .github/workflows/            # ci.yml, release.yml, pages.yml
```

### 2.2 Core dependencies

| Need | Crate | Why |
|---|---|---|
| zip | `zip` (v2+, features: `deflate-zlib-rs`, `deflate64`, `bzip2`, `zstd`, `lzma`, `xz`, `aes-crypto`) | Mature, covers all methods |
| Deflate fast path | `libdeflater` | Decodes a whole buffer 2–3× faster than streaming zlib; use it for entries ≤ 64 MiB with a known size |
| 7z | `sevenz-rust2` | Pure Rust, supports LZMA/LZMA2/BCJ/AES. If it fails a corpus case, vendor the public-domain LZMA SDK C decoder through `cc` and keep the Rust container parser |
| rar | `unrar` (bindings to the official UnRAR source) | Only legal option. Feature-gate it as `rar`, and check the UnRAR license notice into `THIRD_PARTY_LICENSES` |
| tar | `tar` | |
| gzip | `flate2` (backend `zlib-rs`); `gzp` for parallel compression | |
| xz | `liblzma` (multi-threaded decoder and encoder) | |
| zstd | `zstd` (feature `zstdmt`) | |
| bzip2 | `bzip2` (pure-Rust `libbz2-rs-sys` backend) | |
| Checksums | `crc32fast`, `sha2`, `blake3` | SIMD |
| I/O | `memmap2`, `rayon`, `crossbeam-channel` | |
| CLI | `clap` (derive) | |
| Windows API | `windows` (official) | COM, shell, file APIs |
| Errors | `thiserror` (lib), `anyhow` (bin only) | |

---

## 3. tear-core design

### 3.1 Public API (stable surface for the CLI and UI)

```rust
pub struct Archive { /* opened, indexed archive */ }

impl Archive {
    pub fn open(path: &Path, opts: &OpenOptions) -> Result<Archive, TearError>;
    pub fn format(&self) -> Format;
    pub fn entries(&self) -> &[Entry];               // index only, no decoding
    pub fn needs_password(&self) -> bool;
    pub fn plan_extract(&self, dest: &Path, mode: ExtractMode,
                        filter: Option<&EntryFilter>) -> Result<ExtractPlan, TearError>;
}

pub enum ExtractMode { Smart, Into(PathBuf), Here }

pub fn extract(plan: ExtractPlan, ctl: &JobControl) -> Result<ExtractReport, TearError>;
pub fn create(spec: CreateSpec, ctl: &JobControl) -> Result<CreateReport, TearError>;
pub fn test(archive: &Archive, ctl: &JobControl) -> Result<TestReport, TearError>;
pub fn checksum(paths: &[PathBuf], algo: HashAlgo, ctl: &JobControl) -> Result<Vec<Digest>, TearError>;

pub struct JobControl { /* atomic progress counters, cancel + pause flags,
                           callbacks for password, conflict, error decisions */ }
```

- Format detection uses magic bytes first and the file extension only as a tiebreaker. Multi-volume sets (`.7z.001`, `.part1.rar`, `.z01`) open from any volume.
- `JobControl` callbacks are the only way the core asks the user anything. The core never touches the UI.

### 3.2 Smart extract (default for "Extract here")

1. If every entry shares a single top-level directory, extract into the destination as-is, so you get `holiday-photos/`.
2. Otherwise, create `<archive stem>/` and extract into it. For `photos.tar.gz` the stem is `photos`, so strip both extensions.
3. If the target folder already exists, use `name (2)`, `name (3)` and so on. Never merge silently.

"Extract to `name\`" always creates the folder, which is 7-Zip's behaviour.

### 3.3 Extraction pipeline (where the speed comes from)

1. **Index:** open the archive through `mmap` (fall back to `pread` on network drives and for files > 2^47 bytes) and parse the central directory or headers into a compact `Vec<Entry>`.
2. **Plan:** sanitize every path (section 6), resolve conflicts, and compute the full directory set. Create all directories in one pass, sorted and deduplicated, before writing any file.
3. **Decode:**
   - *Independent entries* (zip, non-solid 7z, multiple 7z folders): a rayon parallel iterator over entries, sorted by compressed offset to keep reads sequential per thread. Each worker gets its own reader (a cheap clone over the shared mmap).
   - *Solid streams* (solid 7z folder, tar.*): one decoder thread per stream feeds `(path, bytes)` chunks to the writer pool through a bounded channel. Use the multi-threaded decoders where the format allows it (xz multi-block, zstd multi-frame).
   - *Small entries* (≤ 64 MiB, known size): decode into a pooled buffer in one call with `libdeflater` or a slice-based decoder. Larger entries stream in 4 MiB chunks.
4. **Write** through a dedicated writer pool of `min(cores, 8)` threads:
   - create, preallocate (`SetFileInformationByHandle(FileAllocationInfo)`, `fallocate`, `F_PREALLOCATE`) for files ≥ 1 MiB, write, set times and attributes **through the open handle**, then close.
   - Closing happens on the pool, never on the decode thread. On Windows, `CloseHandle` is where Defender scans, and moving it off the hot path is the biggest single win for many small files.
   - No `fsync`.
5. **Verify:** compute CRC32 or the format checksum inline while writing. A mismatch marks the entry as damaged in the report but does not stop the job.
6. **Finish:** apply directory timestamps last, deepest first, because writing files resets them.

### 3.4 Creation pipeline

- Walk the inputs in parallel (`jwalk` or rayon plus `read_dir`) and sort deterministically.
- zip: compress entries in parallel into temporary buffers or spill files, then write sequentially with the central directory. Default level is Deflate 6 via `libdeflater`.
- 7z: LZMA2 multi-threaded, level 5 by default, solid blocks of up to 64 MiB × threads.
- tar.zst: zstd level 3 with `zstdmt` (all cores) by default.
- Write to `name.ext.tear-tmp` and atomically rename it on success. Delete the temporary file on cancel or failure.

### 3.5 Metadata fidelity

- Keep modification times everywhere. Keep Unix permission bits and symlinks on macOS and Linux. Keep the read-only and hidden attributes on Windows.
- **Mark of the Web (Windows):** if the archive has a `Zone.Identifier` stream, copy it to every extracted file, as Explorer's built-in zip support does. This is a security requirement and is on by default.
- Filename encoding: UTF-8 if the zip flag says so. Otherwise use the Info-ZIP Unicode extra field if present, else detect (CP437 vs. Shift-JIS vs. GBK through `chardetng`), else CP437.

---

## 4. CLI

```
tear x <archive> [-o DIR] [--here | --smart] [-p PASSWORD] [--overwrite|--skip|--rename]
tear a <output.{zip,7z,tar.zst,...}> <paths...> [-l LEVEL] [-p PASSWORD] [--threads N]
tear l <archive> [--json]
tear t <archive>
tear hash <paths...> [--algo crc32|sha256|blake3] [--copy]
tear open <archive>                # archive browser window
tear integrate [--install|--remove] # per-user shell integration (Linux, dev builds)
```

- Exit codes: 0 ok, 1 partial (some entries failed), 2 bad input, 3 wrong or missing password, 4 cancelled.
- `--json` for `l`, `t` and `hash` gives a stable schema documented in `docs/cli-json.md`.
- With no TTY and `--gui` (which shell integrations always pass), run the job runner with lazy UI (section 5.3).

---

## 5. User experience spec

### 5.1 Context menu: exact items

On one or more archives:

```
tear  ›  Extract here
         Extract to "holiday-photos\"        (macOS/Linux: "holiday-photos/")
         Extract to…
         Open in tear
         Test archive
```

On any file(s) or folder(s), archives included:

```
tear  ›  Compress to "holiday-photos.zip"
         Compress to "holiday-photos.7z"
         Compress…
         Copy checksum  ›  SHA-256 · CRC32 · BLAKE3
```

Rules:
- One top-level entry ("tear", with the app icon). Everything else lives in the submenu.
- When an archive is selected, the extract group comes first, then a separator, then the compress group.
- When several items are selected, the name is the parent folder's name ("Compress to "Documents.zip"").
- Use no ellipsis except on items that open a dialog ("Extract to…", "Compress…").
- Users can hide any item in Settings.
- Dropped from 7-Zip: "Compress and email", the duplicate "Open archive" entries, and the separate CRC SHA menu, which is merged into "Copy checksum".

### 5.2 Windows

- **Windows 11 modern menu:** an `IExplorerCommand` COM server (`tear_shell.dll`, from `crates/tear-shell-windows`) with `ECF_HASSUBCOMMANDS`, registered through a **sparse MSIX package** (package identity with an external location) using `desktop4:FileExplorerContextMenus` and `com:SurrogateServer`. The installer signs and registers the sparse package.
- **Classic menu (Windows 10, and "Show more options" on 11):** registry verbs under `HKCU\Software\Classes\` (`SystemFileAssociations\.<ext>\shell\tear`, `*\shell\tear`, `Directory\shell\tear`) using `ExtendedSubCommandsKey`.
- The DLL must stay lightweight: no allocator-heavy dependencies and no core linking. `Invoke` builds a command line and calls `CreateProcessW` for `tear.exe`. `GetState` must return within 1 ms, so it decides from file extensions only and never opens files.
- Multi-select: pass all paths in one invocation.
- Optional file associations (off by default, offered on the installer's last page): double-clicking an archive opens the tear browser.

### 5.3 Progress, conflicts and errors

- **Lazy window:** the job starts immediately. The progress window is created only if the job is still running after **400 ms**. It shows one line ("Extracting holiday-photos.zip"), a bar, the transfer rate and the time left, plus **Pause** and **Cancel**. It closes itself on success. There are no completion toasts or sounds.
- **Conflicts:** one dialog with **Replace**, **Skip** and **Keep both**, plus an "Apply to all remaining" checkbox. Show the name, size and date of both files.
- **Passwords:** one field with a show/hide toggle. "Wrong password" is shown inline under the field; never close and reopen the dialog.
- **Errors** are specific and say what happened and what to do next, for example: "IMG_0412.jpg is damaged in holiday-photos.zip. The other 311 files were extracted." Buttons: **Show files**, **Close**. Never apologize, and never show a raw error code without explanation.
- **Disk full:** stop, delete the partial file, and say how much space is needed.

### 5.4 Archive browser ("Open in tear")

- Single pane: a breadcrumb path, a list (name, size, packed, modified, method) and a minimal toolbar (**Extract**, **Add**, **Test**).
- Double-click a folder to enter it. Double-click a file to extract it to a temp folder and open it with the system handler.
- Drag out to extract (virtual file drop on Windows through `IDataObject` with `CFSTR_FILEDESCRIPTOR`, file promises on macOS). Drag in to add (zip, 7z and tar only).
- Keyboard: Enter opens, Backspace goes up, Ctrl/Cmd+E extracts the selection, and typing selects by name.
- Load time: the index of a 100k-entry zip appears in under 300 ms.

### 5.5 Settings (one small window)

- Context menu: a checkbox per item.
- Default "Compress to" formats (two slots, defaulting to .zip and .7z).
- "Extract here" behaviour: Smart (default) or Always into the current folder.
- File associations (Windows and Linux).
- Settings are stored as plain TOML in the OS config directory (`dirs` crate).

### 5.6 Visual language

The UI follows the platform: Fluent on Windows, system font and accent color, light and dark mode. It uses no custom chrome, no gradients and no illustrations. The app icon is the tear mark (`website/favicon.svg`): a box with a torn-off strip. Use the accent `#2D5BE3` only for the progress bar and the default button.

---

## 6. Security and safety (non-negotiable)

1. **Path traversal (zip-slip):** reject entries that contain `..` components, absolute paths, drive letters, UNC prefixes, or NUL bytes. Normalize separators. Check the resolved path is inside the destination **after** joining.
2. **Symlinks:** never follow a symlink created earlier in the same extraction when writing later entries. Create symlinks last, and only when their target resolves inside the destination; otherwise write them as plain files with a warning. On Windows, skip symlink creation unless Developer Mode is on.
3. **Windows names:** escape reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`), trailing dots and spaces, and the characters `<>:"|?*`. Use the `\\?\` prefix for paths longer than 260 characters.
4. **Case collisions** on case-insensitive filesystems are treated as conflicts, not silent overwrites.
5. **Compression bombs:** warn, and ask whether to continue, when the expansion ratio exceeds 1000:1 *and* the output exceeds 10 GiB. Enforce a hard limit on nesting for recursive formats.
6. **Free space:** check before starting when the uncompressed size is known.
7. **Fuzzing:** a `cargo-fuzz` target per format reader runs in CI nightly for 10 minutes each. Fixed crashes become regression fixtures.
8. **Signing:** every release binary is code-signed (section 9). Releases include `SHA256SUMS` signed with minisign, with the public key in the README.
9. **No network access** except an explicit "Check for updates" button in About, which reads only the GitHub releases API.

---

## 7. Performance plan and benchmarks

### 7.1 Harness

`crates/tear-bench` generates or downloads the corpus and runs `hyperfine` (warmup 2, runs 10) for each tool and case. It writes `bench/results/<os>-<date>.json` and a Markdown table.

### 7.2 Corpus

| Case | Content | Why |
|---|---|---|
| A: many small files | The Linux kernel source tree (~90k files, ~1.4 GB) packed as zip, 7z (solid) and tar.zst | File creation overhead, antivirus cost |
| B: one large file | A 4 GB VM disk image packed as 7z and tar.xz | Raw decoder throughput |
| C: mixed media | 2 GB of JPEG and MP4 as zip (stored and deflate) | I/O-bound path, store method |
| D: create | Case A source → zip and 7z | Parallel compression |

### 7.3 Compared against

7-Zip 24.x/25.x (`7z.exe` / `7zz`), the Windows Explorer built-in zip support, `bsdtar`, Info-ZIP `unzip`, and macOS Archive Utility (timed with a stopwatch script).

### 7.4 Targets (release blockers)

- Matches or beats 7-Zip on every case, on every OS.
- Case A zip on Windows 11 with Defender real-time protection **on** is at least 1.5× faster than 7-Zip.
- Peak memory is under 512 MiB when extracting, except for 7z dictionaries that need more.
- Time to first byte is within the targets in section 1.2, measured with ETW on Windows and `dtrace`/`perf` on the others.

### 7.5 Rules

- Benchmark before and after every performance change and put the numbers in the PR.
- Check binary size in CI: no more than 15 MB for the stripped Windows x64 `tear.exe`.

---

## 8. Platform integration details

### 8.1 macOS

- An Xcode project in `integrations/macos` builds `tear.app`, a thin Swift wrapper that embeds the Rust `tear` binary at `Contents/MacOS/tear-engine`, plus `TearFinder.appex`, a **Finder Sync extension** with `directoryURLs = ["/"]` so the menu appears everywhere.
- The extension is sandboxed, as Apple requires. It never touches files: it sends the action and paths to the main app through the `tear://` URL scheme (or an XPC Mach service if the URL scheme activates the app visibly). The main app is `LSUIElement` (no Dock icon) for menu actions and shows a Dock icon only for the browser window.
- On first launch, show a single onboarding screen with a button that opens System Settings › General › Login Items & Extensions so the user can enable the Finder extension. Do this once only.
- Fallback when the extension is disabled: register NSServices ("Extract with tear", "Compress with tear"). They appear under Quick Actions.
- Distribution: a universal binary (arm64 + x86_64), Developer ID signed, hardened runtime, **notarized and stapled**, in a DMG. Minimum macOS version: 13.

### 8.2 Linux

| File manager | Mechanism | Installed to |
|---|---|---|
| Nautilus (GNOME Files) 43+ | `nautilus-python` `MenuProvider` with a submenu | `/usr/share/nautilus-python/extensions/tear.py` |
| Caja (MATE) | `caja-python` | `/usr/share/caja-python/extensions/tear.py` |
| Dolphin (KDE 6) | KIO service menu with `X-KDE-Submenu=tear` (must be executable) | `/usr/share/kio/servicemenus/tear.desktop` |
| Nemo (Cinnamon) | `.nemo_action` files | `/usr/share/nemo/actions/` |
| Thunar (Xfce) | Custom actions in `uca.xml`, which is per-user | Merged by `tear integrate --install` on first run |

- `tear.desktop` declares `MimeType=` for every supported archive type, plus `share/mime/packages/tear.xml` for types the system lacks (`application/zstd`, `.tzst`).
- Packages: `.deb` (Debian/Ubuntu), `.rpm` (Fedora/openSUSE), and an AppImage (no system-wide integration; offers `tear integrate`). Flatpak is deferred because its sandbox blocks file-manager extensions.
- Recommend `python3-nautilus` / `nautilus-python` as an optional dependency.

### 8.3 Windows packaging

- An **MSI** (WiX v5) per architecture (x64, arm64) that installs per-machine to `Program Files\tear`. It registers the classic verbs, registers and signs the sparse MSIX package for the Windows 11 menu, and restarts no processes (Explorer picks up the menu without a restart).
- Silent install: `msiexec /i tear-windows-x64.msi /qn`.
- Uninstall removes everything, including the HKCU verbs and the sparse package.
- Signing: Azure Trusted Signing (or an EV code-signing certificate) through `signtool` in CI. The sparse package must be signed with the same publisher as its manifest.
- Publish to **winget** (`Xyst69.tear`) on each release.

---

## 9. CI/CD and releases

### 9.1 `ci.yml` (every push and PR)

- Matrix: `windows-latest`, `windows-11-arm`, `macos-latest`, `ubuntu-latest`, `ubuntu-24.04-arm`.
- Steps: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, the corpus round-trip tests, `cargo deny check` (licenses and advisories), and the binary size check.

### 9.2 `release.yml` (on tag `v*`)

Build, sign, package, notarize, generate `SHA256SUMS` and its minisign signature, create the GitHub Release with notes from `CHANGELOG.md`, then open PRs to winget-pkgs and the Homebrew cask.

### 9.3 Release asset names (contract with the website; never change them)

```
tear-windows-x64.msi
tear-windows-arm64.msi
tear-macos-universal.dmg
tear-linux-x64.deb
tear-linux-x64.rpm
tear-linux-x64.AppImage
tear-linux-arm64.deb
tear-linux-arm64.rpm
SHA256SUMS
SHA256SUMS.minisig
```

The website (`website/main.js`) reads `https://api.github.com/repos/Xyst69/tear/releases/latest` and matches assets by these exact names to fill in links and sizes.

### 9.4 Versioning

SemVer. Keep `CHANGELOG.md` in Keep a Changelog format. Ship `v0.x` betas first and `v1.0.0` once section 7.4 passes on all three operating systems.

---

## 10. Phases and exit criteria

These run in order.

**Phase 1: Foundation**
Workspace, toolchain pin, CI matrix green on an empty skeleton, `cargo deny` config, issue templates, `CONTRIBUTING.md`.
*Exit:* CI green on all five runners.

**Phase 2: Core reading and extraction**
Format detection, readers for zip, tar.*, single-stream formats, 7z and rar; the planner with smart extract; the full safety layer from section 6; a corpus of at least 60 fixture archives, including malicious ones (zip-slip, symlink escape, bombs, reserved names, wrong CRC, truncated files).
*Exit:* every fixture extracts or fails with the expected error on all operating systems, and the fuzz targets run 10 minutes each with no crash.

**Phase 3: CLI**
`x`, `l`, `t` and `hash` commands, exit codes, `--json`.
*Exit:* integration tests cover every flag, and `tear l --json` matches its documented schema.

**Phase 4: Speed**
The parallel scheduler, writer pool, `libdeflater` fast path, preallocation, and the benchmark harness.
*Exit:* section 7.4 targets met for extraction, with results committed.

**Phase 5: Creation**
`tear a` for zip, 7z and tar.*, with AES for zip and 7z, and atomic temporary files.
*Exit:* round-trip tests (create with tear, extract with 7-Zip and bsdtar, and the reverse) pass, and case D beats 7-Zip.

**Phase 6: UI**
Lazy progress window, conflict, password and error dialogs, the archive browser, settings, and the About window.
*Exit:* manual QA checklist (section 11) passes on all three operating systems in light and dark mode.

**Phase 7: Windows integration**
COM shell extension, sparse package, classic verbs, MSI, signing.
*Exit:* menu items appear in both the Windows 11 modern menu and the Windows 10 menu, `GetState` p99 is under 1 ms, and install and uninstall leave no residue (checked with a registry diff).

**Phase 8: macOS integration**
Swift wrapper, Finder Sync extension, NSServices fallback, onboarding, notarized DMG.
*Exit:* clean install on macOS 13, 14 and 15 shows the menu after enabling the extension, and Gatekeeper reports no warnings.

**Phase 9: Linux integration**
Integrations for all five file managers, `.desktop` and MIME files, deb, rpm and AppImage.
*Exit:* verified on Ubuntu 24.04 (Nautilus), Fedora 41 KDE (Dolphin), Linux Mint (Nemo), Xubuntu (Thunar) and Ubuntu MATE (Caja).

**Phase 10: Release**
`release.yml`, checksums, winget and Homebrew PRs. Confirm the website's download buttons resolve to real assets.
*Exit:* tag `v0.1.0`, every asset in section 9.3 is present, and every website button downloads the right file.

**Phase 11: Hardening and beta**
Crash reporting is opt-in only (off by default, with local minidumps the user can attach to an issue). Triage beta issues and repeat the benchmark on fresh machines.
*Exit:* `v1.0.0`.

---

## 11. Manual QA checklist (run per OS before each release)

- [ ] Right-click a zip, 7z, rar and tar.zst file: the menu shows one "tear" entry with the correct submenu.
- [ ] Extract here on an archive with a single top-level folder produces no extra wrapper folder.
- [ ] Extract here on loose files creates `<name>/`, and a second extract creates `<name> (2)/`.
- [ ] A job under 400 ms shows no window at all.
- [ ] A 4 GB extract shows progress; Pause, Resume and Cancel work, and cancel leaves no partial files.
- [ ] Wrong password shows the error inline and keeps the dialog open.
- [ ] A damaged archive extracts everything recoverable and the report names the damaged files.
- [ ] Zip-slip and symlink-escape fixtures write nothing outside the destination.
- [ ] Mark of the Web is propagated (Windows): an extracted `.exe` from a downloaded zip shows the SmartScreen prompt.
- [ ] Compress 3 selected files: the archive is named after the parent folder, and the temporary file is cleaned up on cancel.
- [ ] Copy checksum puts the hex digest on the clipboard.
- [ ] Dark mode and high contrast are legible, and every dialog is fully keyboard-operable.
- [ ] Uninstall removes all menu entries without a reboot or Explorer restart.

---

## 12. Licensing

- tear's own code: **MIT OR Apache-2.0** (dual), unless the owner decides otherwise before v0.1.0.
- The UnRAR license allows extraction only and forbids building a RAR compressor from it. Keep `rar` as a cargo feature so distributors can build without it.
- Slint: the royalty-free desktop license requires attribution in the About window.
- `cargo deny` enforces an allowlist of licenses: MIT, Apache-2.0, BSD-2/3, Zlib, ISC, Unicode-3.0, MPL-2.0, and UnRAR (exception for the `unrar_sys` crate only).

---

## 13. Definition of done for v1.0.0

All exit criteria in section 10 are met, the section 7.4 targets pass and their results are committed, the section 11 checklist is signed off on Windows 11, Windows 10, macOS 15, macOS 13, Ubuntu 24.04 and Fedora 41, all assets are signed, and the website's download buttons serve the v1.0.0 files.
