(() => {
  if (window.__raydropDebug) return;

  let liveActive = false;
  let origConsoleError = null;
  let resourceObserver = null;
  let clsObserver = null;
  let liveCls = 0;

  function getCssSelector(el) {
    if (!el || el.nodeType !== 1) return "unknown";
    if (el.id) return `#${el.id}`;
    const tag = el.tagName.toLowerCase();
    const cls = typeof el.className === "string"
      ? el.className.trim().split(/\s+/).filter(Boolean).slice(0, 2).join(".")
      : "";
    return cls ? `${tag}.${cls}` : tag;
  }

  function collectNavigationTiming() {
    const nav = performance.getEntriesByType("navigation")[0];
    if (!nav) return null;
    const dns = Math.max(0, Math.round(nav.domainLookupEnd - nav.domainLookupStart));
    const tcp = Math.max(0, Math.round(nav.connectEnd - nav.connectStart));
    const tls = nav.secureConnectionStart > 0
      ? Math.max(0, Math.round(nav.connectEnd - nav.secureConnectionStart))
      : 0;
    const ttfb = Math.max(0, Math.round(nav.responseStart - nav.requestStart));
    const download = Math.max(0, Math.round(nav.responseEnd - nav.responseStart));
    const domInteractive = Math.max(0, Math.round(nav.domInteractive - nav.startTime));
    const loadTotal = Math.max(0, Math.round((nav.loadEventEnd || nav.domComplete || nav.responseEnd) - nav.startTime));

    return {
      dns,
      tcp,
      tls,
      ttfb,
      download,
      domInteractive,
      loadTotal,
      protocol: nav.nextHopProtocol || "unknown",
      transferSize: nav.transferSize || 0,
      encodedBodySize: nav.encodedBodySize || 0,
      decodedBodySize: nav.decodedBodySize || 0
    };
  }

  function collectWebVitals() {
    const fcpEntry = performance.getEntriesByName("first-contentful-paint")[0];
    const fcp = fcpEntry ? Math.round(fcpEntry.startTime) : null;

    let lcp = null;
    try {
      const lcpEntries = performance.getEntriesByType("largest-contentful-paint");
      if (lcpEntries && lcpEntries.length > 0) {
        lcp = Math.round(lcpEntries[lcpEntries.length - 1].startTime);
      }
    } catch (_) {}

    let cls = 0;
    try {
      const shiftEntries = performance.getEntriesByType("layout-shift");
      if (shiftEntries) {
        for (const entry of shiftEntries) {
          if (!entry.hadRecentInput) cls += entry.value;
        }
      }
    } catch (_) {}

    return {
      fcp,
      lcp,
      cls: Number(cls.toFixed(4))
    };
  }

  function collectResources() {
    const entries = performance.getEntriesByType("resource") || [];
    const byType = {
      script: { count: 0, size: 0, duration: 0 },
      css: { count: 0, size: 0, duration: 0 },
      img: { count: 0, size: 0, duration: 0 },
      font: { count: 0, size: 0, duration: 0 },
      api: { count: 0, size: 0, duration: 0 },
      other: { count: 0, size: 0, duration: 0 }
    };

    const list = [];
    let totalTransfer = 0;
    let cachedCount = 0;

    for (const r of entries) {
      const init = (r.initiatorType || "other").toLowerCase();
      let bucket = "other";
      if (init === "script") bucket = "script";
      else if (init === "link" || init === "css") bucket = "css";
      else if (init === "img" || init === "image") bucket = "img";
      else if (init === "font") bucket = "font";
      else if (init === "fetch" || init === "xmlhttprequest") bucket = "api";

      const size = r.transferSize || r.encodedBodySize || 0;
      const dur = Math.max(0, Math.round(r.duration || 0));
      byType[bucket].count += 1;
      byType[bucket].size += size;
      byType[bucket].duration += dur;
      totalTransfer += size;
      if (r.transferSize === 0 && r.decodedBodySize > 0) cachedCount += 1;

      list.push({
        url: r.name,
        type: bucket,
        duration: dur,
        size,
        cached: r.transferSize === 0 && r.decodedBodySize > 0,
        startTime: Math.round(r.startTime || 0)
      });
    }

    list.sort((a, b) => b.duration - a.duration);

    return {
      totalRequests: entries.length,
      totalTransfer,
      cachedCount,
      byType,
      slowest: list.slice(0, 20)
    };
  }

  function collectUiAudit() {
    const clientWidth = document.documentElement.clientWidth || window.innerWidth;
    const scrollWidth = document.documentElement.scrollWidth;
    const hasOverflow = scrollWidth > clientWidth + 1;

    const viewportMetaEl = document.querySelector('meta[name="viewport"]');
    const viewportMeta = viewportMetaEl ? viewportMetaEl.getAttribute("content") || "" : null;

    const overflowElements = [];
    if (hasOverflow) {
      const all = document.body ? document.body.querySelectorAll("*") : [];
      for (let i = 0; i < all.length && overflowElements.length < 15; i++) {
        const el = all[i];
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) continue;
        if (rect.right > clientWidth + 2 || rect.left < -2) {
          overflowElements.push({
            selector: getCssSelector(el),
            width: Math.round(rect.width),
            overflowPx: Math.round(Math.max(rect.right - clientWidth, Math.abs(Math.min(0, rect.left))))
          });
        }
      }
    }

    const brokenImages = [];
    const oversizedImages = [];
    const missingAltImages = [];
    const imgs = document.querySelectorAll("img");
    for (const img of imgs) {
      const rect = img.getBoundingClientRect();
      if (img.complete && img.naturalWidth === 0 && img.getAttribute("src")) {
        brokenImages.push({ selector: getCssSelector(img), src: img.currentSrc || img.getAttribute("src") });
      } else if (rect.width > 0 && img.naturalWidth > rect.width * 2.2 && img.naturalWidth > 400) {
        oversizedImages.push({
          selector: getCssSelector(img),
          natural: `${img.naturalWidth}x${img.naturalHeight}`,
          rendered: `${Math.round(rect.width)}x${Math.round(rect.height)}`
        });
      }
      if (!img.hasAttribute("alt")) {
        missingAltImages.push(getCssSelector(img));
      }
    }

    let smallTextCount = 0;
    const textEls = document.querySelectorAll("p, span, a, li, td, label, button");
    const sampleLimit = Math.min(textEls.length, 250);
    for (let i = 0; i < sampleLimit; i++) {
      const el = textEls[i];
      if (!el.textContent || !el.textContent.trim()) continue;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const fs = parseFloat(getComputedStyle(el).fontSize);
      if (fs > 0 && fs < 12) smallTextCount++;
    }

    const smallTapTargets = [];
    const interactive = document.querySelectorAll('a, button, input:not([type="hidden"]), select, [role="button"]');
    for (let i = 0; i < interactive.length && smallTapTargets.length < 15; i++) {
      const el = interactive[i];
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      if (rect.width < 44 && rect.height < 44) {
        smallTapTargets.push({
          selector: getCssSelector(el),
          size: `${Math.round(rect.width)}x${Math.round(rect.height)}`
        });
      }
    }

    const isHttps = location.protocol === "https:";
    const mixedContent = [];
    if (isHttps) {
      const media = document.querySelectorAll("img[src], script[src], link[href], iframe[src]");
      for (const node of media) {
        const ref = node.getAttribute("src") || node.getAttribute("href") || "";
        if (ref.startsWith("http://")) {
          mixedContent.push(ref);
        }
      }
    }

    return {
      viewportWidth: clientWidth,
      viewportHeight: window.innerHeight,
      scrollWidth,
      hasOverflow,
      overflowPx: Math.max(0, scrollWidth - clientWidth),
      overflowElements,
      viewportMeta,
      brokenImages: brokenImages.slice(0, 15),
      oversizedImages: oversizedImages.slice(0, 15),
      missingAltCount: missingAltImages.length,
      smallTextCount,
      smallTapTargets,
      mixedContent: mixedContent.slice(0, 15),
      totalImages: imgs.length,
      domNodes: document.getElementsByTagName("*").length
    };
  }

  function runAudit() {
    return {
      url: location.href,
      title: document.title || location.hostname,
      isHttps: location.protocol === "https:",
      navigation: collectNavigationTiming(),
      vitals: collectWebVitals(),
      resources: collectResources(),
      ui: collectUiAudit(),
      timestamp: Date.now()
    };
  }

  function checkOverflowOnly() {
    const clientWidth = document.documentElement.clientWidth || window.innerWidth;
    const scrollWidth = document.documentElement.scrollWidth;
    const hasOverflow = scrollWidth > clientWidth + 1;
    let offenderCount = 0;
    if (hasOverflow && document.body) {
      const all = document.body.querySelectorAll("*");
      for (let i = 0; i < all.length; i++) {
        const rect = all[i].getBoundingClientRect();
        if (rect.width > 0 && (rect.right > clientWidth + 2 || rect.left < -2)) {
          offenderCount++;
        }
      }
    }
    return {
      viewportWidth: clientWidth,
      viewportHeight: window.innerHeight,
      scrollWidth,
      hasOverflow,
      overflowPx: Math.max(0, scrollWidth - clientWidth),
      offenderCount
    };
  }

  function emitLiveEvent(kind, data) {
    if (!liveActive) return;
    try {
      chrome.runtime.sendMessage({
        type: "RAYDROP_DEBUG_PAGE_EVENT",
        kind,
        data,
        timestamp: Date.now()
      });
    } catch (_) {}
  }

  function onWindowError(ev) {
    emitLiveEvent("console_error", {
      level: "error",
      message: ev.message || "Uncaught error",
      source: ev.filename ? `${ev.filename}:${ev.lineno || 0}` : "window.onerror"
    });
  }

  function onUnhandledRejection(ev) {
    const reason = ev.reason instanceof Error ? ev.reason.message : String(ev.reason || "Unhandled rejection");
    emitLiveEvent("console_error", {
      level: "error",
      message: `Unhandled Promise: ${reason}`,
      source: "unhandledrejection"
    });
  }

  function startLive() {
    if (liveActive) return;
    liveActive = true;
    liveCls = 0;

    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);

    if (!origConsoleError) {
      origConsoleError = console.error;
      console.error = function (...args) {
        if (liveActive) {
          const msg = args.map((a) => (typeof a === "object" ? JSON.stringify(a) : String(a))).join(" ");
          emitLiveEvent("console_error", {
            level: "console.error",
            message: msg.slice(0, 500),
            source: location.pathname
          });
        }
        return origConsoleError.apply(console, args);
      };
    }

    try {
      resourceObserver = new PerformanceObserver((list) => {
        for (const r of list.getEntries()) {
          if (r.initiatorType === "fetch" || r.initiatorType === "xmlhttprequest" || r.duration > 400) {
            emitLiveEvent("resource_timing", {
              url: r.name,
              type: r.initiatorType || "other",
              duration: Math.round(r.duration || 0),
              size: r.transferSize || 0
            });
          }
        }
      });
      resourceObserver.observe({ entryTypes: ["resource"] });
    } catch (_) {}

    try {
      clsObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (!entry.hadRecentInput) {
            liveCls += entry.value;
            emitLiveEvent("vital_update", { cls: Number(liveCls.toFixed(4)) });
          }
        }
      });
      clsObserver.observe({ entryTypes: ["layout-shift"] });
    } catch (_) {}
  }

  function stopLive() {
    liveActive = false;
    window.removeEventListener("error", onWindowError);
    window.removeEventListener("unhandledrejection", onUnhandledRejection);
    if (origConsoleError) {
      console.error = origConsoleError;
      origConsoleError = null;
    }
    if (resourceObserver) {
      try { resourceObserver.disconnect(); } catch (_) {}
      resourceObserver = null;
    }
    if (clsObserver) {
      try { clsObserver.disconnect(); } catch (_) {}
      clsObserver = null;
    }
  }

  window.__raydropDebug = {
    runAudit,
    checkOverflowOnly,
    startLive,
    stopLive
  };
})();
