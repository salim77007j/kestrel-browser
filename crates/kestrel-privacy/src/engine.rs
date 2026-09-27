//! Wrapper around Brave's `adblock` engine (adblock-rust).
//!
//! One engine instance holds all rules: network filters, cosmetic filters,
//! exceptions and scriptlets. The Tauri shell consults it for every
//! network request (before it leaves the machine) and for per-page
//! cosmetic CSS injection.

use adblock::engine::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

#[derive(Debug, Clone, Default)]
pub struct CosmeticResult {
    /// CSS selectors to hide on the page.
    pub hide_css: Vec<String>,
    /// Scriptlet / procedural injection script for the page.
    pub injected_script: String,
}

pub struct PrivacyEngine {
    engine: Engine,
    rule_count: usize,
}

impl PrivacyEngine {
    /// Build an engine from a full filter text (all lists concatenated).
    pub fn from_rules(rules: &[String]) -> Result<Self, String> {
        let mut set = FilterSet::new(false);
        let text = rules.join("\n");
        set.add_filter_list(text, ParseOptions::default());
        let engine = Engine::new_with_filter_set(set);
        Ok(Self {
            engine,
            rule_count: rules.len(),
        })
    }

    pub fn rule_count(&self) -> usize {
        self.rule_count
    }

    /// Network-level decision: should this request be blocked?
    ///
    /// `request_type` is a CPT-like token: document, script, image,
    /// stylesheet, xmlhttprequest, media, websocket, other ...
    pub fn should_block(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        match Request::new(url, source_url, request_type, "get") {
            Ok(req) => self.engine.check_network_request(&req).should_block(),
            Err(_) => false,
        }
    }

    /// Does this engine have cosmetic rules for the page?
    pub fn cosmetic(&self, url: &str) -> CosmeticResult {
        let res = self.engine.url_cosmetic_resources(url);
        CosmeticResult {
            hide_css: {
                let mut v: Vec<String> = res.hide_selectors.into_iter().collect();
                v.sort();
                v
            },
            injected_script: res.injected_script,
        }
    }

    /// Human-readable explanation for the settings page "test this URL" tool.
    pub fn explain(&self, url: &str, source: &str) -> String {
        let blocked = self.should_block(url, source, "script");
        let c = self.cosmetic(source);
        format!(
            "network-block: {}\ncosmetic rules: {}\nscriptlets: {}",
            blocked,
            c.hide_css.len(),
            if c.injected_script.is_empty() { 0 } else { 1 }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Vec<String> {
        vec![
            "||doubleclick.net^".into(),
            "||googlesyndication.com^".into(),
            "||ads.example.com^".into(),
            "example.com##.sidebar-ad".into(),
            "example.com##.ad-banner".into(),
            "@@||good.example.com^$script".into(),
        ]
    }

    #[test]
    fn blocks_known_ad_urls() {
        let eng = PrivacyEngine::from_rules(&rules()).unwrap();
        assert!(eng.should_block(
            "https://ad.doubleclick.net/x?foo",
            "https://news.test/",
            "script"
        ));
        assert!(eng.should_block(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://news.test/",
            "script"
        ));
        assert!(eng.should_block(
            "https://ads.example.com/banner.png",
            "https://news.test/",
            "image"
        ));
    }

    #[test]
    fn allows_normal_urls_and_exceptions() {
        let eng = PrivacyEngine::from_rules(&rules()).unwrap();
        assert!(!eng.should_block("https://news.test/article", "https://news.test/", "document"));
        // Exception rule wins for the script on good.example.com
        assert!(!eng.should_block(
            "https://good.example.com/lib.js",
            "https://example.com/",
            "script"
        ));
    }

    #[test]
    fn cosmetic_selectors_for_page() {
        let eng = PrivacyEngine::from_rules(&rules()).unwrap();
        let c = eng.cosmetic("https://example.com/page");
        assert!(c.hide_css.iter().any(|s| s.contains("sidebar-ad")));
        assert!(c.hide_css.iter().any(|s| s.contains("ad-banner")));
        let c2 = eng.cosmetic("https://other.org/");
        assert!(c2.hide_css.is_empty());
    }

    #[test]
    fn large_real_world_list_builds_and_blocks() {
        // A slice of EasyList-style rules incl. options and exceptions.
        let big = vec![
            "-ad-300x250.".into(),
            "||taboola.com^$third-party".into(),
            "||outbrain.com^$third-party".into(),
            "||criteo.net^".into(),
            "||facebook.net/en_US/fbevents.js".into(),
            "##[id^='google_ads_']".into(),
            "##div[class^='AdSlot']".into(),
        ];
        let eng = PrivacyEngine::from_rules(&big).unwrap();
        assert!(eng.should_block(
            "https://cdn.taboola.com/libtrc/loader.js",
            "https://site.test/",
            "script"
        ));
        assert!(eng.should_block(
            "https://images.outbrain.com/img.png",
            "https://site.test/",
            "image"
        ));
        let c = eng.cosmetic("https://site.test/");
        assert!(!c.hide_css.is_empty());
    }
}
