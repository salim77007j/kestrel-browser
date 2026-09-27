//! Wrapper around the `adblock` engine (Brave's adblock-rust).
//!
//! One engine instance holds all rules (network + cosmetic + exceptions).
//! Network-level blocking is additionally compiled into WebKit's native
//! content-filter store (see `converter`); this engine is the source of
//! truth for cosmetic filtering and for auditing URLs.

use adblock::engine::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;

#[derive(Debug, Clone, Default)]
pub struct CosmeticResult {
    /// CSS selectors that should be hidden on the page.
    pub hide_css: Vec<String>,
    /// Scriptlet code to inject (JSON-encoded actions / procedural filters).
    pub injected_script: String,
}

pub struct PrivacyEngine {
    engine: Engine,
    rule_count: usize,
}

impl PrivacyEngine {
    pub fn from_rules(rules: &[String]) -> Result<Self, String> {
        let mut set = FilterSet::new(false);
        let text = rules.join("\n");
        set.add_filter_list(text, ParseOptions::default());
        let engine = Engine::new_with_filter_set(set);
        Ok(Self { engine, rule_count: rules.len() })
    }

    pub fn rule_count(&self) -> usize {
        self.rule_count
    }

    /// Network decision: should this request be blocked?
    pub fn should_block(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        match Request::new(url, source_url, request_type, "get") {
            Ok(req) => self.engine.check_network_request(&req).should_block(),
            Err(_) => false,
        }
    }

    /// Cosmetic decision for a page URL.
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

    /// Public helper used by the settings page "test this URL" tool.
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
