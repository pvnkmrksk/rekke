// Links the book back to the Rekkefly website: the title becomes a link
// home, a Download button joins the menu bar, and every page ends with a
// footer. The book is published at <site>/docs/, so the site root is one
// folder above the book's root (path_to_root is set by mdBook).
(function () {
  "use strict";
  var root = typeof path_to_root === "string" ? path_to_root : "";
  var site = root + "../";
  var repo = "https://github.com/pvnkmrksk/rekkefly";

  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) { node.setAttribute(key, attrs[key]); });
    (children || []).forEach(function (child) {
      node.appendChild(typeof child === "string" ? document.createTextNode(child) : child);
    });
    return node;
  }

  var title = document.querySelector(".menu-title");
  if (title) {
    var logo = el("img", { src: root + "favicon.svg", alt: "", width: "26", height: "26" });
    var favicon = document.querySelector('link[rel="icon"]');
    if (favicon) logo.src = favicon.getAttribute("href");
    title.textContent = "";
    title.appendChild(el("a", { class: "rk-home", href: site, title: "Rekkefly home" },
      [logo, "Rekkefly", el("span", { class: "rk-badge" }, ["Docs"])]));
  }

  var right = document.querySelector(".right-buttons");
  if (right) {
    right.appendChild(el("a", { class: "rk-download", href: site + "#download" }, ["Download"]));
  }

  // "Navy" is the branded dark theme.
  var navy = document.getElementById("mdbook-theme-navy");
  if (navy) navy.textContent = "Dark";

  var main = document.querySelector("#mdbook-content main") || document.querySelector("main");
  if (main && !document.querySelector(".rk-footer")) {
    var heading = main.querySelector("h1");
    var page = heading ? heading.textContent.trim() : document.title;
    var issue = repo + "/issues/new?labels=documentation&title=" +
      encodeURIComponent("Docs: " + page) + "&body=" +
      encodeURIComponent("Page: " + location.href + "\n\nWhat is unclear or wrong?\n");
    main.appendChild(el("footer", { class: "rk-footer" }, [
      el("span", {}, ["Rekkefly · MIT License"]),
      el("nav", { "aria-label": "Site" }, [
        el("a", { href: site }, ["Home"]),
        el("a", { href: repo + "/releases/latest" }, ["Releases"]),
        el("a", { href: repo }, ["GitHub"]),
        el("a", { href: issue }, ["Report a problem with this page"])
      ])
    ]));
  }
})();
