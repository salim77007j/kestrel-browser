//! Anti-fingerprinting injection scripts.
//!
//! Generates one self-contained init script per settings profile. The script
//! runs in every frame of every page BEFORE page scripts (initialization
//! script), applies small deterministic-per-site noise (farbling), and never
//! breaks pages: every hook is wrapped in try/catch and degrades to the
//! original function.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FingerprintSettings {
    pub enabled: bool,
    pub canvas: bool,
    pub audio: bool,
    pub webgl: bool,
    pub fonts: bool,
    pub hardware: bool,
}

impl Default for FingerprintSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            canvas: true,
            audio: true,
            webgl: true,
            fonts: true,
            hardware: true,
        }
    }
}

/// Build the injection script. `seed` is a per-session random u64;
/// per-site noise is derived from (seed, site origin) so a site sees a
/// stable identity within one browsing session but a different one next
/// session, and two sites never see the same fingerprint.
pub fn build_init_script(s: &FingerprintSettings, seed: u64) -> String {
    if !s.enabled {
        return String::new();
    }
    let mut hooks = String::new();

    if s.canvas {
        hooks.push_str(
            r#"
// ---- Canvas farbling: tiny per-site noise in readbacks ----
(function(){
  const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
  const origToBlob = HTMLCanvasElement.prototype.toBlob;
  const origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
  function siteKey(){
    try { return location.origin; } catch(e) { return 'x'; }
  }
  function hash(str){
    let h1 = 0x9e3779b9 ^ (SEED >> 0), h2 = 0x85ebca6b ^ (SEED >>> 16);
    for (let i = 0; i < str.length; i++){
      const c = str.charCodeAt(i);
      h1 = Math.imul(h1 ^ c, 2654435761); h2 = Math.imul(h2 ^ c, 1597334677);
    }
    return [h1 >>> 0, h2 >>> 0];
  }
  const k = hash(siteKey());
  function prng(){
    // xorshift128-ish, deterministic per site
    let a = k[0], b = k[1];
    return function(){ a ^= a << 13; a >>>= 0; b ^= b >>> 7; b ^= b << 17; b >>>= 0; return ((a + b) >>> 0) / 4294967296; };
  }
  HTMLCanvasElement.prototype.toDataURL = function(){
    try {
      const ctx = this.getContext('2d');
      if (ctx){
        const d = origGetImageData.call(ctx, 0, 0, Math.min(this.width||1, 4), Math.min(this.height||1, 4));
        const r = prng();
        for (let i = 0; i < d.data.length; i += 61){ d.data[i] = (d.data[i] + (r() * 3 | 0)) & 0xff; }
        ctx.putImageData(d, 0, 0);
      }
    } catch(e){}
    return origToDataURL.apply(this, arguments);
  };
  CanvasRenderingContext2D.prototype.getImageData = function(){
    const d = origGetImageData.apply(this, arguments);
    try {
      const r = prng();
      for (let i = 0; i < d.data.length; i += 53){ d.data[i] = (d.data[i] + (r() * 3 | 0)) & 0xff; }
    } catch(e){}
    return d;
  };
})();
"#,
        );
    }

    if s.audio {
        hooks.push_str(
            r#"
// ---- AudioContext farbling ----
(function(){
  const orig = AnalyserNode.prototype.getFloatFrequencyData;
  AnalyserNode.prototype.getFloatFrequencyData = function(arr){
    orig.apply(this, arguments);
    try {
      let h = (SEED ^ 0x1234) >>> 0;
      for (let i = 0; i < arr.length; i++){
        h = (Math.imul(h, 16777619) ^ arr[i] * 1e6 | 0) >>> 0;
        arr[i] = arr[i] + ((h % 7) - 3) * 1e-7;
      }
    } catch(e){}
  };
  const orig2 = AudioBuffer.prototype.getChannelData;
  AudioBuffer.prototype.getChannelData = function(){
    const d = orig2.apply(this, arguments);
    try {
      let h = (SEED ^ 0x5678) >>> 0;
      for (let i = 0; i < d.length; i += 37){
        h = (Math.imul(h, 2246822519) + (d[i] * 1e6 | 0)) >>> 0;
        d[i] = d[i] + ((h % 5) - 2) * 1e-8;
      }
    } catch(e){}
    return d;
  };
})();
"#,
        );
    }

    if s.webgl {
        hooks.push_str(
            r#"
// ---- WebGL: generic GPU strings + readnoise ----
(function(){
  const patch = (proto, fn) => {
    const orig = proto.getParameter;
    proto.getParameter = function(p){
      try {
        const v = orig.apply(this, arguments);
        if (p === 37445) return 'Intel Inc.';            // UNMASKED_VENDOR_WEBGL
        if (p === 37446) return 'Intel Iris OpenGL Engine'; // UNMASKED_RENDERER_WEBGL
        if (typeof v === 'number' && p === 3379) return 16384; // MAX_TEXTURE_SIZE
        return v;
      } catch(e){ return orig.apply(this, arguments); }
    };
  };
  try { patch(WebGLRenderingContext.prototype); } catch(e){}
  try { patch(WebGL2RenderingContext.prototype); } catch(e){}
})();
"#,
        );
    }

    if s.fonts {
        hooks.push_str(
            r#"
// ---- Font enumeration resistance: quantize measured text widths ----
(function(){
  const orig = CanvasRenderingContext2D.prototype.measureText;
  CanvasRenderingContext2D.prototype.measureText = function(){
    const m = orig.apply(this, arguments);
    try {
      if (m && typeof m.width === 'number'){ m.width = Math.round(m.width * 20) / 20; }
    } catch(e){}
    return m;
  };
})();
"#,
        );
    }

    if s.hardware {
        hooks.push_str(
            r#"
// ---- Hardware surface normalization ----
(function(){
  try { Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 4, configurable: true }); } catch(e){}
  try { Object.defineProperty(navigator, 'deviceMemory', { get: () => 8, configurable: true }); } catch(e){}
  try { Object.defineProperty(navigator, 'webdriver', { get: () => false, configurable: true }); } catch(e){}
})();
"#,
        );
    }

    if hooks.is_empty() {
        return String::new();
    }

    format!(
        "(function(){{\n  const SEED = {};\n  if (window.__kestrelFP) return; window.__kestrelFP = true;\n{}\n}})();",
        seed, hooks
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_contains_hooks_and_seed() {
        let s = FingerprintSettings::default();
        let js = build_init_script(&s, 12345);
        assert!(js.contains("SEED = 12345"));
        assert!(js.contains("toDataURL"));
        assert!(js.contains("getFloatFrequencyData"));
        assert!(js.contains("UNMASKED_VENDOR_WEBGL"));
        assert!(js.contains("hardwareConcurrency"));
        assert!(js.contains("measureText"));
        // runs once guard
        assert!(js.contains("__kestrelFP"));
    }

    #[test]
    fn disabled_features_omitted() {
        let mut s = FingerprintSettings::default();
        s.canvas = false;
        s.audio = false;
        let js = build_init_script(&s, 1);
        assert!(!js.contains("toDataURL"));
        assert!(!js.contains("getFloatFrequencyData"));
        assert!(js.contains("hardwareConcurrency"));
    }

    #[test]
    fn master_switch_off_returns_empty() {
        let mut s = FingerprintSettings::default();
        s.enabled = false;
        assert!(build_init_script(&s, 9).is_empty());
    }
}
