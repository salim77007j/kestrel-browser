// Kestrel Shield — anti-fingerprinting injection (document start, MAIN world).
// Randomizes readback surfaces that fingerprinters rely on while keeping
// rendering fully intact. Guarded against double injection.
(function () {
  "use strict";
  if (window.__kestrelShield) return;
  window.__kestrelShield = true;

  var MODE = "__KESTREL_MODE__"; // "standard" | "strict" (substituted)
  if (MODE === "off") return;

  // ---- deterministic-per-origin PRNG, salted per browsing session ----
  function hashStr(s) {
    var h = 2166136261;
    for (var i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
    return h >>> 0;
  }
  var sessionSalt;
  try {
    sessionSalt = sessionStorage.getItem("__ksalt");
    if (!sessionSalt) {
      var a = new Uint32Array(2); crypto.getRandomValues(a);
      sessionSalt = a[0] + ":" + a[1];
      try { sessionStorage.setItem("__ksalt", sessionSalt); } catch (e) {}
    }
  } catch (e) { sessionSalt = "nofs"; }
  var seed = hashStr(String(location.origin)) ^ hashStr(sessionSalt);
  function prng() { // mulberry32
    seed |= 0; seed = (seed + 0x6D2B79F5) | 0;
    var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  function noise(amp) { return Math.floor((prng() - 0.5) * 2 * amp); }

  // ---- canvas readback noise ----
  function patchCanvas2D() {
    if (!window.CanvasRenderingContext2D) return;
    var proto = CanvasRenderingContext2D.prototype;
    var origGet = proto.getImageData;
    proto.getImageData = function (sx, sy, sw, sh) {
      var img = origGet.call(this, sx, sy, sw, sh);
      try {
        var d = img.data;
        for (var i = 0; i < d.length; i += 4) {
          if (d[i + 3] === 0) continue;
          d[i] = Math.min(255, Math.max(0, d[i] + noise(2)));
          d[i + 1] = Math.min(255, Math.max(0, d[i + 1] + noise(2)));
          d[i + 2] = Math.min(255, Math.max(0, d[i + 2] + noise(2)));
        }
      } catch (e) {}
      return img;
    };
    if (window.HTMLCanvasElement) {
      var toDataURL = HTMLCanvasElement.prototype.toDataURL;
      HTMLCanvasElement.prototype.toDataURL = function () {
        try {
          var ctx = this.getContext("2d");
          if (ctx && this.width && this.height) {
            var img = origGet.call(ctx, 0, 0, Math.min(this.width, 512), Math.min(this.height, 512));
            var d = img.data;
            for (var i = 0; i < d.length; i += 4) {
              d[i] = Math.min(255, d[i] + noise(2));
            }
            ctx.putImageData(img, 0, 0);
          }
        } catch (e) {}
        return toDataURL.apply(this, arguments);
      };
    }
  }

  // ---- WebGL vendor/renderer spoof + readPixels noise ----
  function patchWebGL() {
    var targets = [];
    if (window.WebGLRenderingContext) targets.push(WebGLRenderingContext.prototype);
    if (window.WebGL2RenderingContext) targets.push(WebGL2RenderingContext.prototype);
    targets.forEach(function (proto) {
      var orig = proto.getParameter;
      proto.getParameter = function (p) {
        // UNMASKED_VENDOR_WEBGL=37445, UNMASKED_RENDERER_WEBGL=37446
        if (p === 37445) return "Mozilla";
        if (p === 37446) return "ANGLE (Kestrel, Generic OpenGL)";
        return orig.call(this, p);
      };
      var origRead = proto.readPixels;
      if (origRead) {
        proto.readPixels = function () {
          origRead.apply(this, arguments);
          try {
            var px = arguments[6];
            if (px && px.length) {
              for (var i = 0; i < Math.min(px.length, 1024); i += 4) px[i] = px[i] ^ 1;
            }
          } catch (e) {}
        };
      }
    });
  }

  // ---- audio fingerprint noise ----
  function patchAudio() {
    if (!window.AudioBuffer) return;
    var proto = AudioBuffer.prototype;
    var orig = proto.getChannelData;
    proto.getChannelData = function () {
      var arr = orig.apply(this, arguments);
      try {
        for (var i = 0; i < Math.min(arr.length, 4096); i++) {
          arr[i] += (prng() - 0.5) * 1e-7;
        }
      } catch (e) {}
      return arr;
    };
  }

  // ---- hardware spoofs ----
  function patchHardware() {
    try {
      Object.defineProperty(navigator, "hardwareConcurrency", { get: function () { return 8; }, configurable: true });
      if (navigator.deviceMemory !== undefined) {
        Object.defineProperty(navigator, "deviceMemory", { get: function () { return 8; }, configurable: true });
      }
    } catch (e) {}
  }

  // ---- strict: font metrics + client rects jitter ----
  function patchStrict() {
    try {
      var origMeasure = TextMetrics ? null : null;
      if (window.CanvasRenderingContext2D && window.TextMetrics) {
        var mp = CanvasRenderingContext2D.prototype.measureText;
        CanvasRenderingContext2D.prototype.measureText = function () {
          var m = mp.apply(this, arguments);
          try {
            var j = 1 + (prng() - 0.5) * 0.004;
            Object.defineProperty(m, "width", { value: m.width * j, configurable: true });
          } catch (e) {}
          return m;
        };
      }
      if (window.Element) {
        var gcr = Element.prototype.getClientRects;
        Element.prototype.getClientRects = function () {
          var list = gcr.apply(this, arguments);
          try {
            for (var i = 0; i < list.length; i++) {
              list[i].width += noise(1); list[i].height += noise(1);
            }
          } catch (e) {}
          return list;
        };
        var gbcr = Element.prototype.getBoundingClientRect;
        Element.prototype.getBoundingClientRect = function () {
          var r = gbcr.apply(this, arguments);
          try { r.width += noise(1); r.height += noise(1); } catch (e) {}
          return r;
        };
      }
    } catch (e) {}
  }

  try { patchCanvas2D(); } catch (e) {}
  try { patchWebGL(); } catch (e) {}
  try { patchAudio(); } catch (e) {}
  try { patchHardware(); } catch (e) {}
  if (MODE === "strict") { try { patchStrict(); } catch (e) {} }
})();
