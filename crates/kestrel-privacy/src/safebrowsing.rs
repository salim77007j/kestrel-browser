//! Safe browsing: malware host blocklist (URLhaus) + phishing lookalike
//! heuristics (IDN punycode, brand lookalikes with edit distance).

use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Threat {
    /// Host found on a malware/phishing blocklist.
    MalwareHost,
    /// Domain looks like a well-known brand but is not it.
    PhishingLookalike { brand: String, detail: String },
}

/// Consolidated safe-browsing checker.
pub struct SafeBrowsing {
    /// Exact host match set (URLhaus-style: hostnames of observed malware).
    hosts: HashSet<String>,
    /// Brand -> legitimate domain suffixes.
    brands: Vec<(&'static str, Vec<&'static str>)>,
}

impl Default for SafeBrowsing {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl SafeBrowsing {
    /// `blocklist`: newline-separated hostnames (URLhaus-style, `#` comments).
    pub fn new(blocklist: Vec<String>) -> Self {
        let hosts: HashSet<String> = blocklist
            .into_iter()
            .map(|l| {
                l.trim()
                    .trim_start_matches('.')
                    .trim_end_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        Self {
            hosts,
            brands: Self::default_brands(),
        }
    }

    fn default_brands() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            ("Google", vec!["google.com", "googleusercontent.com"]),
            ("YouTube", vec!["youtube.com", "youtu.be"]),
            ("Facebook", vec!["facebook.com", "fb.com"]),
            ("Instagram", vec!["instagram.com"]),
            ("WhatsApp", vec!["whatsapp.com"]),
            ("Amazon", vec!["amazon.com"]),
            ("Apple", vec!["apple.com", "icloud.com"]),
            ("Microsoft", vec!["microsoft.com", "live.com"]),
            ("Outlook", vec!["outlook.com", "office.com"]),
            ("Netflix", vec!["netflix.com"]),
            ("PayPal", vec!["paypal.com"]),
            ("eBay", vec!["ebay.com"]),
            ("Wikipedia", vec!["wikipedia.org"]),
            ("Twitter", vec!["twitter.com", "x.com"]),
            ("LinkedIn", vec!["linkedin.com"]),
            ("Steam", vec!["steampowered.com", "steamcommunity.com"]),
            ("Binance", vec!["binance.com"]),
            ("Coinbase", vec!["coinbase.com"]),
            ("GitHub", vec!["github.com"]),
            ("Spotify", vec!["spotify.com"]),
        ]
    }

    /// Number of known malware hosts.
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    /// Check a full URL. Returns a threat description when the URL should get
    /// an interstitial warning.
    pub fn check_url(&self, url: &str) -> Option<Threat> {
        let host = host_of(url)?.to_ascii_lowercase();
        if is_ip(&host) {
            return None;
        }

        // 1. Exact / parent-domain blocklist hit
        if self.host_hit(&host) {
            return Some(Threat::MalwareHost);
        }

        // 2. Punycode IDN: decode and look for brand confusion
        if host.contains("xn--") {
            let (decoded, _) = idna::domain_to_unicode(&host);
            if let Some((brand, detail)) = self.lookalike(&decoded) {
                return Some(Threat::PhishingLookalike {
                    brand: brand.to_string(),
                    detail: format!("internationalized domain '{}' decodes to '{}'", host, decoded),
                });
            }
        }

        // 3. ASCII brand lookalikes
        self.lookalike(&host).map(|(brand, detail)| Threat::PhishingLookalike {
            brand: brand.to_string(),
            detail,
        })
    }

    fn host_hit(&self, host: &str) -> bool {
        if self.hosts.contains(host) {
            return true;
        }
        // walk parent domains: sub.evil.com matches list entry evil.com
        let mut cur = host;
        while let Some(idx) = cur.find('.') {
            cur = &cur[idx + 1..];
            if self.hosts.contains(cur) {
                return true;
            }
        }
        false
    }

    fn lookalike(&self, host: &str) -> Option<(&'static str, String)> {
        let registrable = host.trim_start_matches("www.");
        let core = registrable
            .rsplit_once('.')
            .and_then(|(left, _)| left.rsplit_once('.').map_or(Some(left), |(_, c)| Some(c)))
            .unwrap_or(registrable);
        if core.is_empty() {
            return None;
        }
        // examine the full core and each hyphen segment: paypa1-secure.com
        let mut segments: Vec<String> = core.split('-').map(|s| s.to_string()).collect();
        segments.insert(0, core.to_string());
        for (brand, legit) in &self.brands {
            let legit_core = legit[0].split('.').next().unwrap().to_ascii_lowercase();
            // exact brand name in host: legitimate only on brand domains
            if segments
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&legit_core))
            {
                if legit
                    .iter()
                    .any(|d| host == *d || host.ends_with(&format!(".{}", d)))
                {
                    return None;
                }
                return Some((
                    brand,
                    format!("'{}' uses the {} name outside {} domains", host, brand, legit[0]),
                ));
            }
            for seg in &segments {
                let dist = levenshtein(&seg.to_ascii_lowercase(), &legit_core);
                // threshold scales with the SHORTER string: g00gle(6) matches
                // google within 2, notgoogle(9) needs 3 -> not flagged
                let max = legit_core.len().min(seg.len()) / 4 + 1;
                if dist > 0 && dist <= max {
                    return Some((
                        brand,
                        format!("'{}' is {} edit(s) from '{}'", seg, dist, brand),
                    ));
                }
            }
        }
        None
    }
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let end = rest.find(['/', '?', ':', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn is_ip(host: &str) -> bool {
    host.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sb() -> SafeBrowsing {
        SafeBrowsing::new(vec![
            "malware-host.example".into(),
            "# comment".into(),
            "phishy.io".into(),
        ])
    }

    #[test]
    fn detects_blocklist_hosts() {
        let s = sb();
        assert!(matches!(
            s.check_url("http://malware-host.example/payload.bin"),
            Some(Threat::MalwareHost)
        ));
        assert!(matches!(
            s.check_url("http://sub.phishy.io/x"),
            Some(Threat::MalwareHost)
        ));
        assert!(s.check_url("https://example.com").is_none());
        assert!(s.check_url("https://notphishy.io/").is_none());
    }

    #[test]
    fn detects_brand_lookalikes() {
        let s = SafeBrowsing::default();
        assert!(matches!(
            s.check_url("https://g00gle.com/login"),
            Some(Threat::PhishingLookalike { .. })
        ));
        assert!(matches!(
            s.check_url("https://paypa1-secure.com/x"),
            Some(Threat::PhishingLookalike { .. })
        ));
        assert!(matches!(
            s.check_url("https://arnazon.com"),
            Some(Threat::PhishingLookalike { .. })
        ));
        assert!(s.check_url("https://google.com").is_none());
        assert!(s.check_url("https://www.google.com/search").is_none());
        assert!(s.check_url("https://notgoogle.com").is_none());
        assert!(s.check_url("https://127.0.0.1:8080").is_none());
    }

    #[test]
    fn levenshtein_works() {
        assert_eq!(levenshtein("g00gle", "google"), 2);
        assert_eq!(levenshtein("arnazon", "amazon"), 2);
        assert_eq!(levenshtein("github", "github"), 0);
    }
}
