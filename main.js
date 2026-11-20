(() => {
  "use strict";

  const REPO = "Xyst69/tear";
  const RELEASES = `https://github.com/${REPO}/releases`;

  /* ---------- Platform detection and primary button ---------- */

  const PRIMARY = {
    windows: { asset: "tear-windows-x64.msi", arm: "tear-windows-arm64.msi", label: "Download for Windows", meta: "Windows 10 and 11, 64-bit installer." },
    macos:   { asset: "tear-macos-universal.dmg", label: "Download for macOS", meta: "macOS 13 or later, Apple silicon and Intel." },
    linux:   { asset: "tear-linux-x64.deb", arm: "tear-linux-arm64.deb", label: "Download for Linux", meta: ".deb for Debian and Ubuntu. Other packages below." },
  };

  function detectOS() {
    const ua = navigator.userAgent || "";
    const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/android|iphone|ipad|ipod/i.test(ua)) return null;
    if (/win/i.test(platform) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(platform) || /mac os x/i.test(ua)) return "macos";
    if (/linux|x11/i.test(platform) || /linux/i.test(ua)) return "linux";
    return null;
  }

  async function detectArm() {
    try {
      if (navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
        const v = await navigator.userAgentData.getHighEntropyValues(["architecture"]);
        return v.architecture === "arm";
      }
    } catch (_) { /* not available */ }
    return /aarch64|arm64/i.test(navigator.userAgent || "");
  }

  const os = detectOS();
  const primary = document.getElementById("primary-download");
  const primaryMeta = document.getElementById("primary-meta");
  let primaryAsset = null;

  function assetLink(name) {
    return document.querySelector(`a[data-asset="${name}"]`);
  }

  function syncPrimary() {
    if (!primaryAsset) return;
    const link = assetLink(primaryAsset);
    if (link) primary.href = link.href;
  }

  if (os) {
    const p = PRIMARY[os];
    primaryAsset = p.asset;
    primary.textContent = p.label;
    primaryMeta.textContent = p.meta;
    syncPrimary();

    const column = document.querySelector(`.platform[data-os="${os}"] h3`);
    if (column) {
      const tag = document.createElement("span");
      tag.className = "here";
      tag.textContent = "Your system";
      column.appendChild(tag);
    }

    if (p.arm) {
      detectArm().then((isArm) => {
        if (!isArm) return;
        primaryAsset = p.arm;
        primaryMeta.textContent = os === "windows" ? "Windows 11 on ARM installer." : ".deb for ARM64. Other packages below.";
        syncPrimary();
      });
    }
  }

  /* ---------- Latest release from GitHub ---------- */

  const status = document.getElementById("release-status");
  const download = document.getElementById("download");

  function formatSize(bytes) {
    const mb = bytes / (1024 * 1024);
    return mb >= 10 ? `${Math.round(mb)} MB` : `${mb.toFixed(1)} MB`;
  }

  function formatDate(iso) {
    try {
      return new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
    } catch (_) {
      return "";
    }
  }

  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((res) => {
      if (res.status === 404) return { none: true };
      if (!res.ok) throw new Error(String(res.status));
      return res.json();
    })
    .then((release) => {
      if (release.none) {
        download.classList.add("pending");
        status.innerHTML = "";
        status.append("The first release hasn't been published yet. ");
        const watch = document.createElement("a");
        watch.href = `https://github.com/${REPO}`;
        watch.textContent = "Watch the repository";
        status.append(watch, " to hear when it is.");
        document.querySelectorAll("a[data-asset]").forEach((a) => {
          a.href = RELEASES;
          a.querySelector(".size").textContent = "Not yet available";
          a.classList.add("missing");
        });
        primary.href = "#download";
        return;
      }

      const assets = new Map((release.assets || []).map((a) => [a.name, a]));
      document.querySelectorAll("a[data-asset]").forEach((a) => {
        const asset = assets.get(a.dataset.asset);
        const size = a.querySelector(".size");
        if (asset) {
          a.href = asset.browser_download_url;
          size.textContent = formatSize(asset.size);
        } else {
          a.href = release.html_url || RELEASES;
          size.textContent = "Not in this release";
          a.classList.add("missing");
        }
      });
      syncPrimary();

      status.innerHTML = "";
      const notes = document.createElement("a");
      notes.href = release.html_url || RELEASES;
      notes.textContent = `Version ${String(release.tag_name || "").replace(/^v/, "")}`;
      const date = formatDate(release.published_at);
      status.append(notes, date ? `, released ${date}.` : ".");
    })
    .catch(() => {
      /* Offline or rate-limited: the static /releases/latest/download/ links still work. */
    });

  /* ---------- Hero demo ---------- */

  const demo = document.getElementById("demo");
  const wrap = document.getElementById("stage-wrap");
  const stage = document.getElementById("stage");
  const replay = document.getElementById("replay");
  const STAGE_W = 520;
  const STAGE_H = 340;

  function fit() {
    const s = Math.min(1, wrap.clientWidth / STAGE_W);
    stage.style.setProperty("--s", s.toFixed(4));
    wrap.style.height = `${Math.round(STAGE_H * s)}px`;
  }
  fit();
  if ("ResizeObserver" in window) new ResizeObserver(fit).observe(wrap);
  else window.addEventListener("resize", fit);

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  function play() {
    if (reduceMotion.matches) return;
    demo.classList.remove("play");
    void demo.offsetWidth; // restart the CSS animations
    demo.classList.add("play");
  }

  if (!reduceMotion.matches) {
    replay.hidden = false;
    replay.addEventListener("click", play);
    const start = () => setTimeout(play, 350);
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(start);
    else start();
  }
})();
