(() => {
  "use strict";

  const REPO = "Xyst69/tear";
  const RELEASES = `https://github.com/${REPO}/releases`;
  const WINDOWS_ASSET = "tear-windows-x64.zip";

  /* ---------- Platform detection and primary button ---------- */

  function detectOS() {
    const ua = navigator.userAgent || "";
    const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/android|iphone|ipad|ipod/i.test(ua)) return null;
    if (/win/i.test(platform) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(platform) || /mac os x/i.test(ua)) return "macos";
    if (/linux|x11/i.test(platform) || /linux/i.test(ua)) return "linux";
    return null;
  }

  const os = detectOS();
  const primary = document.getElementById("primary-download");
  const primaryMeta = document.getElementById("primary-meta");

  if (os === "windows") {
    primary.textContent = "Download for Windows";
    primaryMeta.textContent = "Free preview for Windows 10 and 11, 64-bit.";
  } else {
    primary.textContent = "Get the Windows preview";
    primaryMeta.textContent = "Tear runs on Windows for now. macOS and Linux are planned.";
  }

  if (os) {
    const column = document.querySelector(`.platform[data-os="${os}"] h3`);
    if (column) {
      const tag = document.createElement("span");
      tag.className = "here";
      tag.textContent = "Your system";
      column.appendChild(tag);
    }
  }

  function syncPrimary() {
    const link = document.querySelector(`a[data-asset="${WINDOWS_ASSET}"]`);
    if (link) primary.href = link.href;
  }

  /* ---------- Newest release from GitHub (previews included) ---------- */

  const status = document.getElementById("release-status");

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

  function showPending() {
    status.textContent = "";
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
  }

  // /releases/latest skips pre-releases, so read the list and take the newest published one.
  fetch(`https://api.github.com/repos/${REPO}/releases?per_page=10`, { headers: { Accept: "application/vnd.github+json" } })
    .then((res) => {
      if (!res.ok) throw new Error(String(res.status));
      return res.json();
    })
    .then((list) => {
      const release = (Array.isArray(list) ? list : []).find((r) => !r.draft);
      if (!release) {
        showPending();
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

      status.textContent = "";
      const notes = document.createElement("a");
      notes.href = release.html_url || RELEASES;
      const version = String(release.tag_name || "").replace(/^v/, "");
      notes.textContent = release.prerelease ? `Preview ${version}` : `Version ${version}`;
      const date = formatDate(release.published_at);
      status.append(notes, date ? `, released ${date}.` : ".");
    })
    .catch(() => {
      /* Offline or rate-limited: links fall back to the releases page. */
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
