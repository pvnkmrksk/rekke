// Rekkefly website: theme, navigation, downloads from the latest release,
// real-footage videos with their measurements, tabs, copy buttons and reveals.
// Plain JavaScript, no dependencies.
(function () {
  "use strict";
  var root = document.documentElement;
  var reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  var redraws = [];
  root.classList.add("js-ready");

  function $(selector, scope) { return (scope || document).querySelector(selector); }
  function $$(selector, scope) { return Array.prototype.slice.call((scope || document).querySelectorAll(selector)); }
  function store(key, value) {
    try {
      if (value === undefined) return localStorage.getItem(key);
      localStorage.setItem(key, value);
    } catch (e) { return null; }
  }
  function css(name) { return getComputedStyle(root).getPropertyValue(name).trim(); }

  // ---------------------------------------------------------------- theme
  function isDark() { return root.dataset.theme ? root.dataset.theme === "dark" : darkQuery.matches; }
  function themeChanged() { redraws.forEach(function (f) { f(); }); }
  var toggle = $("#theme-toggle");
  if (toggle) {
    toggle.addEventListener("click", function () {
      root.dataset.theme = isDark() ? "light" : "dark";
      store("rekkefly-theme", root.dataset.theme);
      themeChanged();
    });
  }
  if (darkQuery.addEventListener) darkQuery.addEventListener("change", themeChanged);

  // ----------------------------------------------------------- navigation
  var nav = $(".nav");
  function onScroll() { if (nav) nav.classList.toggle("scrolled", window.scrollY > 8); }
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();
  var menuButton = $("#menu-toggle"), menu = $("#mobile-menu");
  if (menuButton && menu) {
    menuButton.addEventListener("click", function () {
      var open = menu.hidden;
      menu.hidden = !open;
      menuButton.setAttribute("aria-expanded", String(open));
    });
    $$("a", menu).forEach(function (a) {
      a.addEventListener("click", function () { menu.hidden = true; menuButton.setAttribute("aria-expanded", "false"); });
    });
  }

  // --------------------------------------------------------------- repo
  // On <owner>.github.io/<repo>/ the repository is derived from the address,
  // so a fork's site links to the fork.
  var repo = "pvnkmrksk/rekkefly";
  var host = location.hostname.match(/^([a-z0-9-]+)\.github\.io$/i);
  var firstSegment = location.pathname.split("/").filter(Boolean)[0];
  if (host && firstSegment && !/\.html?$/.test(firstSegment)) repo = host[1] + "/" + firstSegment;
  var repoUrl = "https://github.com/" + repo;
  $$("[data-repo-link]").forEach(function (a) { a.href = repoUrl + (a.getAttribute("data-repo-link") || ""); });

  // ------------------------------------------------------------ downloads
  function detectOS() {
    var ua = navigator.userAgent || "";
    var platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
    if (/iPhone|iPad|iPod|Android/i.test(ua)) return "mobile";
    if (/Mac/i.test(platform) || /Mac OS X/.test(ua)) return "mac";
    if (/Win/i.test(platform) || /Windows/.test(ua)) return "windows";
    if (/Linux|X11|CrOS/i.test(platform + " " + ua)) return "linux";
    return "other";
  }
  var os = detectOS();
  var names = { mac: "Mac", "mac-intel": "Mac", windows: "Windows", linux: "Ubuntu" };
  var notes = {
    mac: 'Open the DMG and drag Rekkefly onto Applications. If macOS says it "cannot be opened", run <code>xattr -dr com.apple.quarantine "/Applications/Rekkefly.app"</code> once, or see <a href="docs/getting-started/install.html#mac">Mac first launch</a>. Not sure which Mac you have? Apple menu → About This Mac: "Chip: Apple M…" is Apple silicon.',
    windows: 'Run the installer; no administrator rights are needed. The app is not code-signed, so SmartScreen may ask: choose <b>More info → Run anyway</b>. <a href="docs/getting-started/install.html#windows">Windows details</a>.',
    linux: 'Install with <code>sudo apt install ./rekkefly_*_amd64.deb</code> and add yourself to the <code>video</code> group for cameras. Other distributions: the portable tarball. <a href="docs/getting-started/install.html#ubuntu-and-other-linux">Linux details</a>.'
  };
  var release = null;

  function markMine(kind) {
    $$(".dl").forEach(function (card) { card.classList.toggle("mine", card.getAttribute("data-os") === kind); });
    var note = $("[data-install-note]");
    var key = kind === "mac-intel" ? "mac" : kind;
    if (note && notes[key]) { note.innerHTML = notes[key]; note.hidden = false; }
    var label = $("[data-hero-label]");
    if (label && names[kind]) label.textContent = "Download for " + names[kind];
    updateHero(kind);
  }
  function assetFor(card) {
    if (!release || !release.assets) return null;
    var pattern = new RegExp(card.getAttribute("data-asset"));
    for (var i = 0; i < release.assets.length; i++) if (pattern.test(release.assets[i].name)) return release.assets[i];
    return null;
  }
  function updateHero(kind) {
    var button = $("#hero-download");
    if (!button) return;
    var card = $('.dl[data-os="' + kind + '"]');
    var asset = card && kind !== "mac" ? assetFor(card) : null;
    // Apple silicon or Intel cannot always be told from a browser: Macs
    // choose from the cards unless the browser said which.
    button.href = asset ? asset.browser_download_url : "#download";
  }
  function applyRelease() {
    if (!release) return;
    var version = (release.tag_name || "").replace(/^v/, "");
    $$("[data-release-version]").forEach(function (el) { el.textContent = "v" + version; });
    $$(".dl").forEach(function (card) {
      var asset = assetFor(card);
      if (!asset) return;
      card.href = asset.browser_download_url;
      var file = $("[data-file]", card), size = $("[data-size]", card);
      if (file) file.textContent = asset.name;
      if (size) size.textContent = (asset.size / 1e6).toFixed(0) + " MB";
    });
    var line = $("[data-release-line]");
    var date = release.published_at ? new Date(release.published_at) : null;
    if (line && version) {
      line.textContent = "Rekkefly " + version + (date ? ", released " + date.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" }) : "") +
        ". Each download includes the Bogong moth example and the example configs.";
    }
    var meta = $("[data-hero-meta]");
    if (meta && version) meta.textContent = "Free and open source · v" + version + " · macOS · Windows · Linux";
    var mine = $(".dl.mine");
    if (mine) updateHero(mine.getAttribute("data-os"));
  }
  function loadRelease() {
    var key = "rekkefly-release:" + repo, cached = null;
    try { cached = JSON.parse(sessionStorage.getItem(key) || "null"); } catch (e) { cached = null; }
    if (cached && Date.now() - cached.at < 30 * 60 * 1000) { release = cached.release; applyRelease(); return; }
    if (!window.fetch) return;
    fetch("https://api.github.com/repos/" + repo + "/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
      .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
      .then(function (data) {
        release = { tag_name: data.tag_name, published_at: data.published_at, assets: (data.assets || []).map(function (a) {
          return { name: a.name, size: a.size, browser_download_url: a.browser_download_url };
        }) };
        try { sessionStorage.setItem(key, JSON.stringify({ at: Date.now(), release: release })); } catch (e) {}
        applyRelease();
      })
      .catch(function () { /* The cards keep linking to the releases page. */ });
  }
  if (names[os]) markMine(os);
  if (os === "mac" && navigator.userAgentData && navigator.userAgentData.getHighEntropyValues) {
    navigator.userAgentData.getHighEntropyValues(["architecture"]).then(function (v) {
      if (v.architecture === "x86") markMine("mac-intel");
    }).catch(function () {});
  }
  loadRelease();

  // -------------------------------------------------------------- reveal
  var observerSupported = "IntersectionObserver" in window;
  function countUp(el) {
    var to = parseFloat(el.getAttribute("data-to"));
    if (reduceMotion || !isFinite(to)) return;
    var start = null, duration = 1100;
    function step(t) {
      if (start === null) start = t;
      var k = Math.min(1, (t - start) / duration);
      el.textContent = Math.round(to * (1 - Math.pow(1 - k, 3)));
      if (k < 1) requestAnimationFrame(step);
    }
    el.textContent = "0";
    requestAnimationFrame(step);
  }
  if (observerSupported && !reduceMotion) {
    var revealer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("in");
        $$(".count", entry.target).forEach(countUp);
        revealer.unobserve(entry.target);
      });
    }, { rootMargin: "0px 0px -8% 0px", threshold: 0.08 });
    $$(".reveal").forEach(function (el, i) {
      el.style.transitionDelay = (i % 4) * 70 + "ms";
      revealer.observe(el);
    });
  } else {
    $$(".reveal").forEach(function (el) { el.classList.add("in"); });
  }

  // ---------------------------------------------------------------- tabs
  $$("[data-tabs]").forEach(function (tabs) {
    var buttons = $$('[role="tab"]', tabs);
    function select(button) {
      buttons.forEach(function (b) {
        var on = b === button;
        b.setAttribute("aria-selected", String(on));
        b.tabIndex = on ? 0 : -1;
        document.getElementById(b.getAttribute("aria-controls")).hidden = !on;
      });
    }
    buttons.forEach(function (button, i) {
      button.addEventListener("click", function () { select(button); });
      button.addEventListener("keydown", function (e) {
        var next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : null;
        if (next === null) return;
        e.preventDefault();
        var target = buttons[(next + buttons.length) % buttons.length];
        select(target); target.focus();
      });
    });
  });

  // ---------------------------------------------------------------- copy
  $$("[data-copy]").forEach(function (button) {
    button.addEventListener("click", function () {
      var text = document.getElementById(button.getAttribute("data-copy")).innerText.trim();
      function done() {
        button.textContent = "Copied";
        button.classList.add("done");
        setTimeout(function () { button.textContent = "Copy"; button.classList.remove("done"); }, 1600);
      }
      if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, function () {});
    });
  });

  // ------------------------------------------------------------- footage
  // The videos are real footage with Rekkefly's tracking drawn on; the
  // numbers beside them are its measurements of the frame on screen
  // (site/assets/moth-angles.json, made with the videos by
  // scripts/web_media.py).
  var angles = null, footage = [];
  function frameOf(video) { return Math.max(0, Math.min(899, Math.floor(video.currentTime * 30 + 0.01))); }
  function signed(v) { return v === null || v === undefined ? "—" : (v >= 0 ? "+" : "−") + Math.abs(v).toFixed(1) + "°"; }
  function diff(a, b) { return a === null || b === null ? null : a - b; }

  function drawTrace(canvas, k) {
    var W = 600, H = 96, ratio = Math.min(window.devicePixelRatio || 1, 2), box = canvas.getBoundingClientRect();
    if (!box.width) return;
    var scale = box.width / W;
    if (canvas.width !== Math.round(W * scale * ratio)) {
      canvas.width = Math.round(W * scale * ratio); canvas.height = Math.round(H * scale * ratio);
    }
    var ctx = canvas.getContext("2d");
    ctx.setTransform(scale * ratio, 0, 0, scale * ratio, 0, 0);
    ctx.clearRect(0, 0, W, H);
    var span = 120, x0 = 12, x1 = W - 12, mid = 56, gain = 0.55;
    ctx.strokeStyle = css("--line-strong"); ctx.lineWidth = 1; ctx.setLineDash([3, 5]);
    ctx.beginPath(); ctx.moveTo(x0, mid); ctx.lineTo(x1, mid); ctx.stroke(); ctx.setLineDash([]);
    var L = angles.tracking.left, R = angles.tracking.right;
    function line(value, color, width, alpha) {
      ctx.beginPath(); ctx.globalAlpha = alpha; var pen = false;
      for (var j = Math.max(0, k - span); j <= k; j++) {
        var v = value(j);
        if (v === null) { pen = false; continue; }
        var x = x1 - (k - j) * (x1 - x0) / span, y = Math.max(4, Math.min(H - 4, mid - v * gain));
        if (pen) ctx.lineTo(x, y); else { ctx.moveTo(x, y); pen = true; }
      }
      ctx.strokeStyle = color; ctx.lineWidth = width; ctx.stroke(); ctx.globalAlpha = 1;
    }
    line(function (j) { return L[j]; }, "#f75d63", 1.2, 0.45);
    line(function (j) { return R[j]; }, "#57d873", 1.2, 0.45);
    line(function (j) { return diff(L[j], R[j]); }, css("--ink"), 2.2, 1);
  }

  function update(box) {
    if (!angles) return;
    var k = frameOf(box.video);
    if (box.kind === "tracking") {
      var L = angles.tracking.left[k], R = angles.tracking.right[k], now = performance.now();
      // Ten updates a second: every frame's number is real, but 30 a second is unreadable.
      if (!box.shown || now - box.shown > 100 || box.video.paused) {
        box.shown = now;
        box.left.textContent = signed(L); box.right.textContent = signed(R); box.diff.textContent = signed(diff(L, R));
      }
      if (box.trace) drawTrace(box.trace, k);
    } else if (box.kind === "antennae") {
      // Count, up to the frame on screen, the frames where the antennae moved
      // each detector's L − R by more than 5° from its own result on the clean frame.
      var clean = angles.antennae.clean, counts = { classic: 0, rekkefly: 0 };
      ["classic", "rekkefly"].forEach(function (name) {
        var values = angles.antennae[name];
        for (var j = 0; j <= k; j++) {
          var reference = name === "classic" ? clean.classic[j] : clean.rekkefly[j];
          if (values[j] === null || reference === null || Math.abs(values[j] - reference) > 5) counts[name]++;
        }
      });
      box.tally.classic.textContent = counts.classic;
      box.tally.rekkefly.textContent = counts.rekkefly;
      box.tally.seen.forEach(function (el) { el.textContent = k + 1; });
    }
  }
  function loop(box) {
    function step() {
      update(box);
      if (!box.video.paused) box.raf = requestAnimationFrame(step); else box.raf = 0;
    }
    if (!box.raf) box.raf = requestAnimationFrame(step);
  }

  $$("[data-footage]").forEach(function (el) {
    var video = $("video", el), button = $(".play", el);
    var box = { el: el, video: video, kind: el.getAttribute("data-footage"), raf: 0 };
    if (box.kind === "tracking") {
      box.left = $('[data-angle="left"]', el); box.right = $('[data-angle="right"]', el); box.diff = $('[data-angle="diff"]', el);
      var figure = el.closest("figure");
      box.trace = figure ? $("[data-trace]", figure) : null;
    } else {
      var figure2 = el.closest("figure");
      box.tally = { classic: $('[data-tally="classic"]', figure2), rekkefly: $('[data-tally="rekkefly"]', figure2), seen: $$('[data-tally="seen"]', figure2) };
    }
    footage.push(box);
    function play() { video.play().catch(function () { el.classList.remove("playing"); }); }
    video.addEventListener("play", function () { el.classList.add("playing"); loop(box); });
    video.addEventListener("pause", function () { el.classList.remove("playing"); update(box); });
    video.addEventListener("seeked", function () { update(box); });
    if (button) button.addEventListener("click", play);
    // Moving pictures only for those who have not asked for less motion,
    // and only while they are on screen.
    if (reduceMotion) { video.removeAttribute("autoplay"); video.pause(); }
    if (observerSupported) {
      new IntersectionObserver(function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting && !reduceMotion) { if (video.preload === "none") video.preload = "auto"; play(); }
          else if (!entry.isIntersecting) video.pause();
        });
      }, { threshold: 0.25 }).observe(el);
    } else if (!reduceMotion) {
      play();
    }
  });
  if (footage.length && window.fetch) {
    fetch("assets/moth-angles.json")
      .then(function (r) { if (!r.ok) throw new Error(r.status); return r.json(); })
      .then(function (data) {
        angles = data;
        footage.forEach(update);
      })
      .catch(function () { $$(".readout").forEach(function (el) { el.hidden = true; }); });
  }
  redraws.push(function () { footage.forEach(update); });
  window.addEventListener("resize", function () { footage.forEach(update); });
})();
