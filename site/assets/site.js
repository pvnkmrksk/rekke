// Rekkefly website: theme, navigation, downloads from the latest release,
// the animated hero, the detector explainer, tabs, copy buttons and reveals.
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

  // --------------------------------------------------------------- video
  $$(".media").forEach(function (box) {
    var video = $("video", box), button = $(".play", box);
    if (!video) return;
    function play() { video.play().then(function () { box.classList.add("playing"); }).catch(function () {}); }
    if (button) button.addEventListener("click", function () { video.controls = true; play(); });
    video.addEventListener("pause", function () { if (!video.controls) box.classList.remove("playing"); });
    if (reduceMotion || !observerSupported || !video.hasAttribute("data-autoplay")) return;
    new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) { video.preload = "auto"; play(); }
        else if (!video.controls) video.pause();
      });
    }, { threshold: 0.35 }).observe(box);
  });

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

  // ------------------------------------------------------ canvas helpers
  function fitCanvas(canvas, width, height) {
    var ratio = Math.min(window.devicePixelRatio || 1, 2);
    var box = canvas.getBoundingClientRect();
    var scale = box.width / width;
    canvas.width = Math.round(width * scale * ratio);
    canvas.height = Math.round(height * scale * ratio);
    var ctx = canvas.getContext("2d");
    ctx.setTransform(scale * ratio, 0, 0, scale * ratio, 0, 0);
    return ctx;
  }
  function whenVisible(element, callback) {
    if (!observerSupported) { callback(true); return; }
    new IntersectionObserver(function (entries) { callback(entries[0].isIntersecting); }).observe(element);
  }
  var deg = Math.PI / 180;

  // ---------------------------------------------------------------- hero
  // A tethered fly seen from above, head up. Wing angles are Kinefly's:
  // 0 perpendicular to the body, positive toward the head. The left wing's
  // image angle is 180° + φ, the right wing's −φ.
  (function hero() {
    var canvas = $("#hero-canvas");
    if (!canvas || !canvas.getContext) return;
    var W = 600, H = 540, ctx = fitCanvas(canvas, W, H);
    var hinges = [{ x: 272, y: 214, side: -1 }, { x: 328, y: 214, side: 1 }];
    var inner = 46, outer = 176, sectorFrom = -55, sectorTo = 76;
    // The animation clock only runs while the hero is drawn, so the trace
    // never jumps after the page was scrolled away or hidden.
    var history = [], lastSample = -1, colors = {}, visible = true, raf = 0, clock = 0, previous = 0;
    var out = { l: $("#ro-l"), r: $("#ro-r"), d: $("#ro-d") }, lastText = 0;

    function readColors() {
      colors = { ink: css("--ink"), muted: css("--muted"), line: css("--line-strong"), wing: css("--wing"), coral: css("--coral"),
        left: css("--left"), right: css("--right"), surface: css("--surface"), dark: isDark() };
    }
    function imageAngle(side, phi) { return (side < 0 ? 180 + phi : -phi) * deg; }
    function point(h, r, phi) { var a = imageAngle(h.side, phi); return [h.x + r * Math.cos(a), h.y + r * Math.sin(a)]; }
    function steer(t) { return 9 * Math.sin(2 * Math.PI * 0.16 * t + 1) + 4 * Math.sin(2 * Math.PI * 0.37 * t); }
    function front(t, side) { return 44 + 9 * Math.sin(2 * Math.PI * 0.11 * t) + (side < 0 ? 0.5 : -0.5) * steer(t); }
    function back() { return -40; }
    // The measured leading edge: the true one plus a little detector noise.
    function measured(t, side) { return front(t, side) + 0.35 * Math.sin(t * 37 + side * 5) * Math.cos(t * 23); }

    function wedge(h, r0, r1, p0, p1) {
      var a0 = imageAngle(h.side, p0), a1 = imageAngle(h.side, p1), ccw = h.side > 0;
      ctx.beginPath();
      ctx.arc(h.x, h.y, r1, a0, a1, ccw);
      ctx.arc(h.x, h.y, r0, a1, a0, !ccw);
      ctx.closePath();
    }
    function leg(points) {
      ctx.beginPath();
      ctx.moveTo(points[0][0], points[0][1]);
      for (var i = 1; i < points.length; i++) ctx.lineTo(points[i][0], points[i][1]);
      ctx.stroke();
    }
    function draw(t, now) {
      ctx.clearRect(0, 0, W, H);
      var alpha = colors.dark ? 0.42 : 0.30;
      // The fly sits below the readout chips; the trace panel is not moved.
      ctx.save();
      ctx.translate(0, 22);

      // Sectors.
      hinges.forEach(function (h) {
        wedge(h, inner, outer, sectorFrom, sectorTo);
        ctx.fillStyle = colors.dark ? "rgba(255,255,255,0.025)" : "rgba(18,35,41,0.025)";
        ctx.fill();
        ctx.setLineDash([6, 5]); ctx.lineWidth = 1.6;
        ctx.strokeStyle = h.side < 0 ? colors.left : colors.right;
        ctx.globalAlpha = 0.75; ctx.stroke(); ctx.globalAlpha = 1; ctx.setLineDash([]);
      });

      // Stroke envelopes (the motion blur) and the beating wing.
      hinges.forEach(function (h) {
        var f = front(t, h.side), b = back();
        var g = ctx.createRadialGradient(h.x, h.y, 10, h.x, h.y, 168);
        g.addColorStop(0, "rgba(31,92,102,0.02)");
        g.addColorStop(0.55, colors.dark ? "rgba(124,200,210," + alpha + ")" : "rgba(31,92,102," + alpha + ")");
        g.addColorStop(1, colors.dark ? "rgba(124,200,210,0.12)" : "rgba(31,92,102,0.12)");
        wedge(h, 12, 168, b, f);
        ctx.fillStyle = g; ctx.fill();
        if (!reduceMotion) {
          var mid = (f + b) / 2, amp = (f - b) / 2;
          for (var k = 0; k < 4; k++) {
            var phi = mid + amp * 0.97 * Math.sin(2 * Math.PI * 3.1 * t - k * 0.13);
            var a = imageAngle(h.side, phi);
            ctx.save();
            ctx.translate(h.x, h.y); ctx.rotate(a);
            ctx.beginPath();
            ctx.ellipse(82, 0, 82, 15, 0, 0, Math.PI * 2);
            ctx.fillStyle = colors.dark ? "rgba(200,230,235," + (0.16 - k * 0.035) + ")" : "rgba(18,35,41," + (0.11 - k * 0.025) + ")";
            ctx.fill();
            ctx.restore();
          }
        }
      });

      // Legs and antennae: they move, and the front legs reach into the
      // sectors now and then. The tracked edges ignore them.
      ctx.strokeStyle = colors.ink; ctx.lineCap = "round"; ctx.lineJoin = "round";
      var reach = Math.max(0, Math.sin(2 * Math.PI * 0.23 * t));
      [-1, 1].forEach(function (s) {
        var kick = 0.5 + 0.5 * Math.sin(2 * Math.PI * 1.3 * t + s);
        ctx.lineWidth = 3;
        leg([[300 + s * 16, 236], [300 + s * (40 + 26 * reach), 214 - 30 * reach], [300 + s * (52 + 60 * reach), 178 - 34 * reach - 6 * kick]]);
        leg([[300 + s * 18, 248], [300 + s * 52, 262], [300 + s * 66, 300 + 4 * kick]]);
        leg([[300 + s * 14, 258], [300 + s * 40, 290], [300 + s * 44, 336 - 3 * kick]]);
        ctx.lineWidth = 2;
        var sway = 4 * Math.sin(2 * Math.PI * 0.6 * t + s);
        leg([[300 + s * 8, 134], [300 + s * 22, 104], [300 + s * (40 + sway), 82]]);
      });

      // Body, eyes and the body axis.
      ctx.fillStyle = colors.ink;
      ctx.beginPath(); ctx.ellipse(300, 302, 24, 62, 0, 0, Math.PI * 2); ctx.fill();
      ctx.beginPath(); ctx.ellipse(300, 222, 34, 40, 0, 0, Math.PI * 2); ctx.fill();
      ctx.beginPath(); ctx.ellipse(300, 150, 26, 20, 0, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = "#8a2e27";
      ctx.beginPath(); ctx.ellipse(284, 148, 10, 13, 0, 0, Math.PI * 2); ctx.fill();
      ctx.beginPath(); ctx.ellipse(316, 148, 10, 13, 0, 0, Math.PI * 2); ctx.fill();
      ctx.strokeStyle = colors.dark ? "rgba(255,255,255,0.14)" : "rgba(255,255,255,0.18)"; ctx.lineWidth = 2;
      for (var s = 0; s < 4; s++) { ctx.beginPath(); ctx.moveTo(282 + s, 282 + s * 18); ctx.quadraticCurveTo(300, 288 + s * 18, 318 - s, 282 + s * 18); ctx.stroke(); }
      ctx.setLineDash([2, 6]); ctx.strokeStyle = colors.muted; ctx.lineWidth = 1.2;
      ctx.beginPath(); ctx.moveTo(300, 60); ctx.lineTo(300, 378); ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = "#d63384"; ctx.beginPath(); ctx.arc(300, 128, 5.5, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = "#22b8cf"; ctx.beginPath(); ctx.arc(300, 366, 5.5, 0, Math.PI * 2); ctx.fill();

      // Tracked leading edges.
      var angles = hinges.map(function (h) { return measured(t, h.side); });
      hinges.forEach(function (h, i) {
        var p0 = point(h, inner, angles[i]), p1 = point(h, outer, angles[i]);
        ctx.strokeStyle = colors.coral; ctx.lineWidth = 4.5; ctx.lineCap = "round";
        ctx.beginPath(); ctx.moveTo(p0[0], p0[1]); ctx.lineTo(p1[0], p1[1]); ctx.stroke();
        ctx.fillStyle = colors.coral; ctx.beginPath(); ctx.arc(h.x, h.y, 5.5, 0, Math.PI * 2); ctx.fill();
        ctx.strokeStyle = colors.surface; ctx.lineWidth = 2; ctx.stroke();
      });
      ctx.restore();

      // The camera samples at 30 fps; keep 6 s of L, R and L − R.
      var frame = Math.floor(t * 30);
      if (frame !== lastSample) {
        lastSample = frame;
        history.push([angles[0], angles[1]]);
        if (history.length > 180) history.shift();
      }
      drawTrace();
      if ((!now || now - lastText > 120) && out.l) {
        lastText = now;
        out.l.textContent = signed(angles[0]); out.r.textContent = signed(angles[1]); out.d.textContent = signed(angles[0] - angles[1]);
      }
    }
    function signed(v) { return (v >= 0 ? "+" : "−") + Math.abs(v).toFixed(1) + "°"; }
    function drawTrace() {
      var x0 = 24, x1 = W - 24, y0 = 420, y1 = 522, mid = (y0 + y1) / 2;
      ctx.fillStyle = colors.dark ? "rgba(0,0,0,0.22)" : "rgba(255,255,255,0.55)";
      roundRect(x0 - 10, y0 - 12, x1 - x0 + 20, y1 - y0 + 24, 14); ctx.fill();
      ctx.setLineDash([3, 5]); ctx.strokeStyle = colors.line; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(x0, mid); ctx.lineTo(x1, mid); ctx.stroke(); ctx.setLineDash([]);
      ctx.font = "600 11px Inter, system-ui, sans-serif"; ctx.fillStyle = colors.muted;
      ctx.fillText("L − R · last 6 s", x0 + 2, y0 + 4);
      function line(value, color, width) {
        ctx.beginPath();
        for (var i = 0; i < history.length; i++) {
          var x = x1 - (history.length - 1 - i) * (x1 - x0) / 179;
          var y = mid - value(history[i]) * 1.15;
          if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
        }
        ctx.strokeStyle = color; ctx.lineWidth = width; ctx.stroke();
      }
      ctx.globalAlpha = 0.5;
      line(function (p) { return p[0] - 44; }, colors.left, 1.4);
      line(function (p) { return p[1] - 44; }, colors.right, 1.4);
      ctx.globalAlpha = 1;
      line(function (p) { return p[0] - p[1]; }, colors.ink, 2.4);
    }
    function roundRect(x, y, w, h, r) {
      ctx.beginPath();
      ctx.moveTo(x + r, y); ctx.arcTo(x + w, y, x + w, y + h, r); ctx.arcTo(x + w, y + h, x, y + h, r);
      ctx.arcTo(x, y + h, x, y, r); ctx.arcTo(x, y, x + w, y, r); ctx.closePath();
    }
    function loop(now) {
      if (previous) clock += Math.min(0.05, (now - previous) / 1000);
      previous = now;
      draw(clock, now);
      raf = visible && !document.hidden ? requestAnimationFrame(loop) : 0;
      if (!raf) previous = 0;
    }
    function start() { if (!raf) raf = requestAnimationFrame(loop); }
    readColors();
    if (reduceMotion) {
      // One still frame, with a trace already drawn.
      for (var i = 0; i < 180; i++) { var tt = i / 30; history.push([measured(tt, -1), measured(tt, 1)]); }
      clock = 6; lastSample = Infinity;
      draw(clock, 0);
    } else {
      whenVisible(canvas, function (v) { visible = v; if (v) start(); });
      document.addEventListener("visibilitychange", function () { if (!document.hidden && visible) start(); });
      start();
    }
    redraws.push(function () { readColors(); if (reduceMotion) draw(clock, 0); });
    window.addEventListener("resize", function () { ctx = fitCanvas(canvas, W, H); if (reduceMotion) draw(clock, 0); });
  })();

  // ----------------------------------------------------------- explainer
  // One radial band of one wing sector: intensity across the sector, with
  // the wing's broad blur and a narrow leg. The classic detector reports the
  // steepest edge; Rekkefly removes narrow features and ranks edges by the
  // shoulder behind them, then takes the leading (head-side) edge.
  (function explainer() {
    var canvas = $("#explainer-canvas");
    if (!canvas || !canvas.getContext) return;
    var W = 900, H = 380, ctx = fitCanvas(canvas, W, H);
    var lo = -50, hi = 70, step = 0.5, n = Math.round((hi - lo) / step) + 1;
    var tailEdge = -34, truth = 38, spike = 5, cap = 0.5, windowDeg = 20;
    var pos = $("#leg-pos"), dark = $("#leg-dark"), animate = $("#leg-animate");
    var posOut = $("#leg-pos-out"), darkOut = $("#leg-dark-out");
    var vClassic = $("#v-classic"), vRekkefly = $("#v-rekkefly");
    var colors = {}, raf = 0, visible = true, last = 0, phase = 0;

    function readColors() {
      colors = { ink: css("--ink"), muted: css("--muted"), line: css("--line"), wing: css("--wing"), coral: css("--coral"), text: css("--text"), dark: isDark() };
    }
    function sigmoid(x) { return 1 / (1 + Math.exp(-x)); }
    function angle(i) { return lo + i * step; }
    // The leg position slider runs 2–118 across the sector; map to degrees.
    function legAngle() { return lo + parseFloat(pos.value); }
    function profile() {
      var legAt = legAngle(), d = parseFloat(dark.value), p = new Array(n);
      for (var i = 0; i < n; i++) {
        var a = angle(i);
        var wing = 0.42 * sigmoid((a - tailEdge) / 1.6) * sigmoid((truth - a) / 1.9);
        var texture = 0.012 * Math.sin(a * 0.9) + 0.008 * Math.sin(a * 2.3 + 1);
        var legv = 0.62 * d * Math.exp(-Math.pow(a - legAt, 2) / (2 * 1.0 * 1.0));
        p[i] = Math.min(1, 0.1 + wing + texture + legv);
      }
      return p;
    }
    function smoothGradient(p) {
      var s = p.map(function (_, i) { var a = Math.max(0, i - 1), b = Math.min(n - 1, i + 1); var sum = 0; for (var j = a; j <= b; j++) sum += p[j]; return sum / (b - a + 1); });
      return s.map(function (_, i) { var a = Math.max(0, i - 2), b = Math.min(n - 1, i + 2); return (s[b] - s[a]) / (b - a); });
    }
    function open(p, r) {
      var eroded = p.map(function (_, i) { var m = Infinity; for (var j = Math.max(0, i - r); j <= Math.min(n - 1, i + r); j++) m = Math.min(m, p[j]); return m; });
      return eroded.map(function (_, i) { var m = -Infinity; for (var j = Math.max(0, i - r); j <= Math.min(n - 1, i + r); j++) m = Math.max(m, eroded[j]); return m; });
    }
    function peaks(g) {
      var out = [];
      for (var i = 1; i < n - 1; i++) { var v = Math.abs(g[i]); if (v > 1e-4 && v >= Math.abs(g[i - 1]) && v > Math.abs(g[i + 1])) out.push(i); }
      return out;
    }
    function classic(p) {
      var g = smoothGradient(p), best = -1;
      peaks(g).forEach(function (i) { if (best < 0 || Math.abs(g[i]) > Math.abs(g[best])) best = i; });
      return best;
    }
    function rekkefly(p) {
      var base = Math.min.apply(null, p);
      var capped = p.map(function (v) { return Math.min(v, base + cap); });
      var opened = open(capped, Math.ceil(spike / step / 2));
      var g = smoothGradient(opened), win = Math.round(windowDeg / step);
      function mean(a, b) { var s = 0, c = 0; for (var j = Math.max(0, a); j < Math.min(n, b); j++) { s += opened[j] - base; c++; } return [c ? s / c : 0, c]; }
      var ranked = peaks(g).map(function (i) {
        var after = mean(i + 1, i + 1 + win), before = mean(i - win, i);
        var complete = Math.min(1, Math.min(after[1], before[1]) * 2 / win);
        var shoulder = complete * (g[i] > 0 ? after[0] - before[0] : before[0] - after[0]);
        return { i: i, score: Math.abs(g[i]) * (Math.max(0, shoulder) + 1e-3 * cap) };
      }).sort(function (a, b) { return b.score - a.score; });
      // The leading edge: wing gives way to background toward the head.
      for (var k = 0; k < ranked.length; k++) if (g[ranked[k].i] < 0) return { i: ranked[k].i, filtered: opened };
      return { i: ranked.length ? ranked[0].i : -1, filtered: opened };
    }
    function x(a) { return 60 + (a - lo) / (hi - lo) * (W - 90); }
    function y(v) { return 300 - v * 205; }
    function marker(a, color, label, dashed, row) {
      var px = x(a);
      ctx.strokeStyle = color; ctx.lineWidth = dashed ? 2 : 3; ctx.setLineDash(dashed ? [6, 5] : []);
      ctx.beginPath(); ctx.moveTo(px, 90); ctx.lineTo(px, 300); ctx.stroke(); ctx.setLineDash([]);
      ctx.font = "700 13px Inter, system-ui, sans-serif";
      var w = ctx.measureText(label).width + 16, lx = Math.min(Math.max(px - w / 2, 4), W - w - 4), ly = 6 + row * 27;
      ctx.fillStyle = color; roundRect(lx, ly, w, 22, 11); ctx.fill();
      ctx.fillStyle = color === colors.coral || color === "#e8a33b" ? "#2a0e09" : (colors.dark ? "#0a1519" : "#fff");
      ctx.fillText(label, lx + 8, ly + 15);
    }
    function roundRect(px, py, w, h, r) {
      ctx.beginPath(); ctx.moveTo(px + r, py); ctx.arcTo(px + w, py, px + w, py + h, r); ctx.arcTo(px + w, py + h, px, py + h, r);
      ctx.arcTo(px, py + h, px, py, r); ctx.arcTo(px, py, px + w, py, r); ctx.closePath();
    }
    function draw() {
      var p = profile(), c = classic(p), r = rekkefly(p);
      ctx.clearRect(0, 0, W, H);
      // Axes.
      ctx.strokeStyle = colors.line; ctx.lineWidth = 1;
      for (var a = -40; a <= 60; a += 20) { ctx.beginPath(); ctx.moveTo(x(a), 90); ctx.lineTo(x(a), 300); ctx.stroke(); }
      ctx.beginPath(); ctx.moveTo(x(lo), 300); ctx.lineTo(x(hi), 300); ctx.stroke();
      ctx.font = "600 12px Inter, system-ui, sans-serif"; ctx.fillStyle = colors.muted; ctx.textAlign = "center";
      for (a = -40; a <= 60; a += 20) ctx.fillText((a > 0 ? "+" : a < 0 ? "−" : "") + Math.abs(a) + "°", x(a), 320);
      ctx.textAlign = "left"; ctx.fillText("← toward the tail", x(lo), 350);
      ctx.textAlign = "right"; ctx.fillText("toward the head →", x(hi), 350);
      ctx.textAlign = "center"; ctx.fillText("wing angle in the sector", x(10), 350);
      ctx.textAlign = "left";
      // Raw profile, filled.
      ctx.beginPath(); ctx.moveTo(x(lo), y(0));
      for (var i = 0; i < n; i++) ctx.lineTo(x(angle(i)), y(p[i]));
      ctx.lineTo(x(hi), y(0)); ctx.closePath();
      ctx.fillStyle = colors.dark ? "rgba(124,200,210,0.10)" : "rgba(31,92,102,0.08)"; ctx.fill();
      ctx.beginPath();
      for (i = 0; i < n; i++) { var px = x(angle(i)), py = y(p[i]); if (i) ctx.lineTo(px, py); else ctx.moveTo(px, py); }
      ctx.strokeStyle = colors.muted; ctx.lineWidth = 1.4; ctx.stroke();
      // What Rekkefly ranks on: narrow features removed.
      ctx.beginPath();
      for (i = 0; i < n; i++) { px = x(angle(i)); py = y(r.filtered[i]); if (i) ctx.lineTo(px, py); else ctx.moveTo(px, py); }
      ctx.strokeStyle = colors.wing; ctx.lineWidth = 3.2; ctx.stroke();
      // Labels for the curves.
      ctx.font = "600 12px Inter, system-ui, sans-serif";
      ctx.fillStyle = colors.wing; ctx.fillText("wing blur, legs removed", x(-30), y(0.53) - 4);
      ctx.fillStyle = colors.muted; ctx.fillText("leg", x(legAngle()) - 9, y(Math.min(1, p[Math.round((legAngle() - lo) / step)])) - 10);
      // Markers.
      marker(truth, colors.ink, "true leading edge", true, 0);
      var classicAngle = angle(c), rekkeflyAngle = angle(r.i);
      var classicClose = Math.abs(classicAngle - rekkeflyAngle) < 3;
      marker(classicAngle, "#e8a33b", "classic", false, classicClose ? 2 : 1);
      marker(rekkeflyAngle, colors.coral, "Rekkefly", false, 1);
      verdict(vClassic, classicAngle); verdict(vRekkefly, rekkeflyAngle);
    }
    function verdict(el, a) {
      var err = Math.abs(a - truth);
      el.textContent = (a >= 0 ? "+" : "−") + Math.abs(a).toFixed(1) + "° · off " + err.toFixed(1) + "°";
      el.className = err > 2 ? "bad" : "good";
    }
    function sync() { posOut.textContent = (legAngle() >= 0 ? "+" : "−") + Math.abs(legAngle()).toFixed(0) + "°"; darkOut.textContent = parseFloat(dark.value).toFixed(2); draw(); }
    function loop(now) {
      if (animate.checked && !reduceMotion) {
        if (last) phase += (now - last) / 1000;
        pos.value = (60 + 56 * Math.sin(phase * 0.55)).toFixed(1);
        sync();
      }
      last = now;
      raf = visible && animate.checked && !document.hidden ? requestAnimationFrame(loop) : 0;
      if (!raf) last = 0;
    }
    function start() { if (!raf && animate.checked && !reduceMotion) raf = requestAnimationFrame(loop); }
    readColors();
    if (reduceMotion) animate.checked = false;
    [pos, dark].forEach(function (input) { input.addEventListener("input", function () { if (input === pos) animate.checked = false; sync(); }); });
    animate.addEventListener("change", function () { if (animate.checked) start(); });
    function dragTo(e) {
      var box = canvas.getBoundingClientRect(), a = lo + ((e.clientX - box.left) / box.width * W - 60) / (W - 90) * (hi - lo);
      pos.value = Math.max(2, Math.min(118, a - lo)); animate.checked = false; sync();
    }
    var dragging = false;
    canvas.addEventListener("pointerdown", function (e) { dragging = true; canvas.setPointerCapture(e.pointerId); dragTo(e); });
    canvas.addEventListener("pointermove", function (e) { if (dragging) dragTo(e); });
    canvas.addEventListener("pointerup", function () { dragging = false; });
    canvas.style.cursor = "ew-resize";
    whenVisible(canvas, function (v) { visible = v; if (v) start(); });
    document.addEventListener("visibilitychange", function () { if (!document.hidden && visible) start(); });
    redraws.push(function () { readColors(); draw(); });
    window.addEventListener("resize", function () { ctx = fitCanvas(canvas, W, H); draw(); });
    sync();
    start();
  })();
})();
