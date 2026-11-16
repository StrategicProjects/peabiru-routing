// Links to the other peabiru sites in the menu bar of the mdBook guides
// (Rust and JavaScript; site/build.sh copies this file to both). The guides
// live one level below the site's root (_site/rust/, _site/js/), and mdBook
// defines path_to_root, the way from a page to its book's root.
(function () {
  "use strict";
  var bar = document.querySelector(".menu-bar .right-buttons");
  if (!bar || typeof path_to_root === "undefined") return;
  var site = path_to_root + "../";
  var title = (document.querySelector(".menu-title") || {}).textContent || "";
  var links = [
    ["Home", "index.html"],
    ["R", "r/index.html"],
    ["Python", "python/index.html"],
    ["Rust", "rust/index.html", "peabiru for Rust"],
    ["JavaScript", "js/index.html", "peabiru for JavaScript"],
    ["Benchmarks", "benchmarks.html"],
  ];
  var nav = document.createElement("nav");
  nav.className = "peabiru-sites";
  nav.setAttribute("aria-label", "peabiru sites");
  links.forEach(function (l) {
    var a = document.createElement("a");
    a.textContent = l[0];
    a.href = site + l[1];
    if (l[2] && title.trim() === l[2]) a.setAttribute("aria-current", "page");
    nav.appendChild(a);
  });
  bar.insertBefore(nav, bar.firstChild);
})();
