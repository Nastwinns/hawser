// Language switcher for the bilingual mdBook (English default, French under /fr/).
// Injects an EN/FR toggle into the top menu bar. The French build is a full
// copy served beneath the docs root at `/fr/`; toggling swaps that segment.
(function () {
  "use strict";

  function counterpart(pathname, toFrench) {
    // Deployed layout: <root>/docs/ (EN) and <root>/docs/fr/ (FR).
    if (/\/docs\//.test(pathname)) {
      return toFrench
        ? pathname.replace(/\/docs\//, "/docs/fr/")
        : pathname.replace(/\/docs\/fr\//, "/docs/");
    }
    // Local `mdbook serve` fallback: root is `/`, French copy under `/fr/`.
    return toFrench
      ? "/fr" + pathname
      : pathname.replace(/^\/fr(\/|$)/, "/");
  }

  function build() {
    var lang = (document.documentElement.lang || "en").toLowerCase();
    var isFrench = lang.indexOf("fr") === 0;

    var bar = document.querySelector(".right-buttons") ||
              document.querySelector(".menu-bar .right-buttons");
    if (!bar) return;
    if (document.querySelector(".lang-switch")) return;

    var a = document.createElement("a");
    a.className = "lang-switch";
    a.title = isFrench ? "Read this page in English" : "Lire cette page en français";
    a.textContent = isFrench ? "EN" : "FR";
    a.href = counterpart(window.location.pathname, !isFrench);
    bar.appendChild(a);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", build);
  } else {
    build();
  }
})();
