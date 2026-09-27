//! Kestrel privacy core.
//!
//! Responsibilities:
//! - Content-blocker rule conversion (ABP/uBlock syntax -> WebKit compiled filters)
//! - Cosmetic filtering decisions (per-page CSS + scriptlets) via the `adblock` engine
//! - Filter-list management (bundled seeds + remote updates)
//! - Fingerprint-randomization script generation
//! - Tracker-audit host sets + heuristics (lookalike domains)

pub mod converter;
pub mod engine;
pub mod fingerprint;
pub mod lists;
pub mod shield;

pub use engine::{CosmeticResult, PrivacyEngine};
pub use fingerprint::FingerprintMode;
pub use lists::{ListId, ListManager, ListStatus};
pub use shield::Shield;

/// User agent string used for our own outbound fetches (filter updates).
pub const KESTREL_UA: &str = "Kestrel/0.1 (+https://github.com/salim77007j/kestrel-browser; privacy-first browser)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_blocks_common_ads() {
        let rules = vec![
            "||doubleclick.net^".to_string(),
            "||googlesyndication.com^".to_string(),
            "/ads/banner.$image".to_string(),
            "@@||doubleclick.net^$image,domain=example.com".to_string(),
        ];
        let eng = PrivacyEngine::from_rules(&rules).expect("engine");
        assert!(eng.should_block("https://ad.doubleclick.net/x", "https://news.test/", "script"));
        // Exception on example.com should NOT block
        assert!(!eng.should_block(
            "https://ad.doubleclick.net/x",
            "https://example.com/",
            "image"
        ));
        assert!(!eng.should_block(
            "https://example.com/main.js",
            "https://example.com/",
            "script"
        ));
    }

    #[test]
    fn cosmetic_hides_selectors() {
        let rules = vec![
            "example.com##.sidebar-ad".to_string(),
            "##div[class^=\"ad-banner\"]".to_string(),
        ];
        let eng = PrivacyEngine::from_rules(&rules).expect("engine");
        let c = eng.cosmetic("https://example.com/page");
        assert!(c.hide_css.iter().any(|s| s.contains("sidebar-ad")));
        let c2 = eng.cosmetic("https://other.org/");
        assert!(c2.hide_css.iter().any(|s| s.contains("ad-banner")));
    }

    #[test]
    fn converter_produces_valid_json_rules() {
        let rules = vec![
            "! comment".to_string(),
            "##.hide-me".to_string(),
            "||ads.example.com^".to_string(),
            "||tracker.net^$script,third-party".to_string(),
            "@@||ok.example.com^$document".to_string(),
            "/path/to/ad^$image".to_string(),
            "example.com^$badfilter".to_string(), // unsupported -> dropped
        ];
        let json = converter::to_content_blocker(&rules, 1000).expect("convert");
        let arr: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(arr.len(), 4); // 3 blocks + 1 exception
    }

    #[test]
    fn tracker_host_extraction() {
        let rules = vec![
            "||trk.example.com^".to_string(),
            "||wide.net^$script".to_string(),
            "@@||good.com^".to_string(),
            "||wild*.net^".to_string(),
        ];
        let hosts = lists::extract_tracker_hosts(&rules);
        assert!(hosts.contains(&"trk.example.com".to_string()));
        assert!(hosts.contains(&"wide.net".to_string()));
        assert!(!hosts.contains(&"good.com".to_string()));
        assert!(!hosts.iter().any(|h| h.contains('*')));
    }
}
