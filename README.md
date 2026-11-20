<p align="center">
  <img src="website/favicon.svg" width="64" height="64" alt="">
</p>

<h1 align="center">tear</h1>

<p align="center">Unpack anything from the right-click menu.<br>A fast, quiet archive tool for Windows, macOS and Linux.</p>

---

Right-click any archive and choose **tear › Extract here**. The files appear immediately, with no splash screen, no pop-ups and nothing left running in the background.

```
tear  ›  Extract here
         Extract to "holiday-photos\"
         Extract to…
         Open in tear
         Test archive
```

## Status

tear is in early development. Nothing has been released yet. [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) is the full engineering plan: architecture, formats, performance targets, shell integration for each OS, and release pipeline.

## Formats

| Format | Extract | Create |
|---|---|---|
| zip | Yes | Yes, with AES-256 |
| 7z | Yes | Yes, with AES-256 |
| rar | Yes | No |
| tar, tar.gz, tar.xz, tar.zst, tar.bz2 | Yes | Yes |
| gz, xz, zst, bz2 | Yes | Yes |

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
website/                 download site (static HTML, CSS, JS)
```
