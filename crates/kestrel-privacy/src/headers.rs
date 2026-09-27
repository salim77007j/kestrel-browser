//! Third-party cookie policy + privacy header logic.
//!
//! Pure decision logic, applied by the network request interceptor in the
//! shell layer.

use serde::{Deserialize, Serialize};

/// Third-party cookie policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CookiePolicy {
    /// Accept all cookies (least private).
    AllowAll,
    /// Block cookies on cross-site requests (default).
    BlockThirdParty,
}

impl Default for CookiePolicy {
    fn default() -> Self {
        Self::BlockThirdParty
    }
}

/// What to do with cookie data on a request/response pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieAction {
    Keep,
    StripRequest,
    StripResponse,
    StripBoth,
}

/// Is this request cross-site (third party) relative to its initiator?
/// Both hosts are full hostnames (case-insensitive, www. ignored).
pub fn is_third_party(target_host: &str, source_host: &str) -> bool {
    let t = registrable_suffix(target_host);
    let s = registrable_suffix(source_host);
    !t.eq_ignore_ascii_case(&s)
}

/// Very small eTLD+1 approximation: last two labels, or three for
/// common two-part public suffixes.
pub fn registrable_suffix(host: &str) -> String {
    let host = host.trim().trim_start_matches("www.").to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return host;
    }
    let two_part_tlds = [
        "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "org.au",
        "co.jp", "or.jp", "ne.jp", "co.nz", "com.br", "com.cn", "com.mx",
        "co.in", "co.za", "com.sg", "com.tr", "com.ar", "com.tw",
    ];
    let last2 = labels[labels.len() - 2..].join(".");
    if two_part_tlds.contains(&last2.as_str()) && labels.len() >= 3 {
        labels[labels.len() - 3..].join(".")
    } else {
        last2
    }
}

/// Apply the cookie policy to a request.
///
/// * `policy` — configured policy
/// * `third_party` — outcome of `is_third_party` for this request
/// * `has_request_cookies` — the request carries a Cookie header
/// * `has_response_set_cookie` — the response carries Set-Cookie
pub fn cookie_action(
    policy: CookiePolicy,
    third_party: bool,
    has_request_cookies: bool,
    has_response_set_cookie: bool,
) -> CookieAction {
    match policy {
        CookiePolicy::AllowAll => CookieAction::Keep,
        CookiePolicy::BlockThirdParty => {
            if !third_party {
                CookieAction::Keep
            } else {
                match (has_request_cookies, has_response_set_cookie) {
                    (true, true) => CookieAction::StripBoth,
                    (true, false) => CookieAction::StripRequest,
                    (false, true) => CookieAction::StripResponse,
                    (false, false) => CookieAction::Keep,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_party_detection() {
        assert!(is_third_party("ads.example.com", "news.site"));
        assert!(is_third_party("www.example.com", "example.org"));
        assert!(!is_third_party("www.example.com", "shop.example.com"));
        assert!(!is_third_party("example.co.uk", "www.example.co.uk"));
        assert!(is_third_party("tracker.co.uk", "example.co.uk"));
    }

    #[test]
    fn registrable_suffix_cases() {
        assert_eq!(registrable_suffix("WWW.Example.COM"), "example.com");
        assert_eq!(registrable_suffix("a.b.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable_suffix("localhost"), "localhost");
        assert_eq!(registrable_suffix("example.com"), "example.com");
    }

    #[test]
    fn cookie_actions() {
        let p = CookiePolicy::BlockThirdParty;
        assert_eq!(cookie_action(p, false, true, true), CookieAction::Keep);
        assert_eq!(
            cookie_action(p, true, true, true),
            CookieAction::StripBoth
        );
        assert_eq!(
            cookie_action(p, true, false, true),
            CookieAction::StripResponse
        );
        assert_eq!(
            cookie_action(p, true, true, false),
            CookieAction::StripRequest
        );
        assert_eq!(cookie_action(CookiePolicy::AllowAll, true, true, true), CookieAction::Keep);
    }
}
