//! ABP/uBlock filter syntax -> WebKitGTK content-blocker JSON (Safari-style).
//!
//! WebKitGTK compiles these rules into its network process (C speed, zero
//! per-request cost in the UI process). We deliberately support the subset of
//! syntax that maps cleanly and *drop* anything ambiguous so compilation of
//! the whole list can never fail.

use serde_json::{json, Value};

/// Convert ABP-style rules into a WebKit content-blocker JSON string.
/// `max_rules` caps output to keep compile time bounded.
pub fn to_content_blocker(rules: &[String], max_rules: usize) -> Result<String, String> {
    let mut out: Vec<Value> = Vec::with_capacity(rules.len().min(max_rules));
    for raw in rules {
        if out.len() >= max_rules {
            break;
        }
        if let Some(rule) = convert_rule(raw) {
            out.push(rule);
        }
    }
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

fn is_element_hiding(r: &str) -> bool {
    r.contains("##") || r.contains("#@#") || r.contains("#?#") || r.contains("#$#")
}

fn is_comment(r: &str) -> bool {
    let t = r.trim();
    t.is_empty() || t.starts_with('!') || t.starts_with("[Adblock") || t.starts_with('#')
}

/// Split `$options` from the pattern (only when the suffix looks like options).
fn split_options(rule: &str) -> (&str, Option<&str>) {
    if let Some(idx) = rule.rfind('$') {
        let (base, opts) = (&rule[..idx], &rule[idx + 1..]);
        if !base.is_empty()
            && !opts.is_empty()
            && opts.chars().all(|c| c.is_ascii_alphanumeric() || "-_,=~|".contains(c))
        {
            return (base, Some(opts));
        }
    }
    (rule, None)
}

fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '*' => out.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Map ABP resource options to Safari/WebKit resource-type strings.
fn resource_types(opts: &[&str]) -> Option<Vec<&'static str>> {
    let mut types = Vec::new();
    for o in opts {
        types.extend(match *o {
            "script" => vec!["script"],
            "image" => vec!["image"],
            "stylesheet" | "css" => vec!["style-sheet"],
            "subdocument" | "document" => vec!["document"],
            "xmlhttprequest" | "xhr" | "websocket" | "other" | "ping" => vec!["raw"],
            "font" => vec!["font"],
            "media" | "object" => vec!["media"],
            "popup" => vec!["pop-up"],
            _ => return None, // unknown -> caller drops the rule
        });
    }
    if types.is_empty() {
        None
    } else {
        Some(types)
    }
}

fn convert_rule(raw: &str) -> Option<Value> {
    let rule = raw.trim();
    if is_comment(rule) || is_element_hiding(rule) {
        return None;
    }

    let exception = rule.starts_with("@@");
    let body = if exception { &rule[2..] } else { rule };

    let (pattern, opts_str) = split_options(body);
    let opts: Vec<&str> = opts_str.map(|o| o.split(',').collect()).unwrap_or_default();

    // Unsupported global-modifier options -> drop rule entirely (safe).
    for o in &opts {
        if matches!(
            *o,
            "important" | "badfilter" | "cookie" | "csp" | "redirect" | "rewrite" | "gh"
                | "genericblock" | "ghide" | "generichide" | "elemhide" | "ehide" | "jsinject"
        ) {
            return None;
        }
    }

    // Domain source restrictions -> if-domain / unless-domain.
    let mut if_domain: Vec<String> = Vec::new();
    let mut unless_domain: Vec<String> = Vec::new();
    let mut load_type: Vec<&str> = Vec::new();
    let mut res_types: Vec<Vec<&str>> = Vec::new();

    for o in &opts {
        if let Some(d) = o.strip_prefix("domain=") {
            for part in d.split('|') {
                if let Some(neg) = part.strip_prefix('~') {
                    if !neg.is_empty() {
                        unless_domain.push(format!("*.{neg}"));
                    }
                } else if !part.is_empty() {
                    if_domain.push(format!("*.{part}"));
                }
            }
        } else if *o == "third-party" {
            load_type.push("third-party");
        } else if *o == "first-party" {
            load_type.push("first-party");
        } else if let Some(rt) = resource_types(std::slice::from_ref(o)) {
            res_types.push(rt);
        } else if !matches!(*o, "match-case") {
            return None; // unknown option -> drop
        }
    }
    let resource_type = res_types.into_iter().flatten().collect::<Vec<_>>();

    // Pattern -> url-filter regex.
    let url_filter = pattern_to_regex(pattern)?;
    if !webkit_safe(&url_filter) {
        return None;
    }

    let mut trigger = json!({ "url-filter": url_filter });
    if !resource_type.is_empty() {
        trigger["resource-type"] = json!(resource_type);
    }
    if !load_type.is_empty() {
        trigger["load-type"] = json!(load_type);
    }
    if !if_domain.is_empty() {
        trigger["if-domain"] = json!(if_domain);
    }
    if !unless_domain.is_empty() {
        trigger["unless-domain"] = json!(unless_domain);
    }

    let action = if exception {
        json!({ "type": "ignore-previous-rules" })
    } else {
        json!({ "type": "block" })
    };

    Some(json!({ "trigger": trigger, "action": action }))
}

fn pattern_to_regex(pattern: &str) -> Option<String> {
    if pattern.is_empty() {
        return None;
    }
    if let Some(rest) = pattern.strip_prefix("||") {
        // host[.tld]^[/path...]
        let anchored_end = rest.ends_with('^');
        let core = rest.trim_end_matches('^');
        let (host, path) = match core.find('/') {
            Some(i) => (&core[..i], Some(&core[i + 1..])),
            None => (core, None),
        };
        if host.is_empty() || host.contains('/') {
            return None;
        }
        let host_re = escape_regex(host);
        let mut re = String::from("^[^:]+://+([^/?#]*\\.)?");
        re.push_str(&host_re);
        if let Some(p) = path {
            re.push_str("(/");
            re.push_str(&escape_regex(p));
            re.push(')');
        }
        if anchored_end {
            // Boundary after the host/path. WebKit's content-filter regex
            // engine rejects disjunctions (`|`), so we cannot write
            // `([/?#:]|$)`. Two safe forms instead:
            //   * bare host: REQUIRED class — WebKit canonicalizes
            //     "https://host" with a trailing `/`, so the char always
            //     exists, and requiring it blocks "host.evil.io" tricks;
            //   * host + path: OPTIONAL class — the path literal anchors the
            //     match, and URLs may legitimately end right after the path.
            if path.is_some() {
                re.push_str("[/?:#]?");
            } else {
                re.push_str("[/?:#]");
            }
        }
        Some(re)
    } else if pattern.starts_with('|') && !pattern.starts_with("||") {
        let mut re = String::from("^");
        re.push_str(&escape_regex(pattern.trim_start_matches('|')));
        Some(re)
    } else {
        // Plain substring pattern (most common in EasyList).
        Some(escape_regex(pattern))
    }
}

/// WebKit compiles the whole store in one shot: a single unsupported regex
/// fails EVERY rule. Fail safe — never emit a pattern the engine rejects.
fn webkit_safe(re: &str) -> bool {
    !re.contains('|') && !re.contains("(?=") && !re.contains("(?!") && !re.contains("\\1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_anchor_regex() {
        let re = pattern_to_regex("||ads.example.com^").unwrap();
        let json = convert_rule("||ads.example.com^").unwrap();
        assert_eq!(json["action"]["type"], "block");
        assert!(re.starts_with("^[^:]+://"));
        // WebKit rejects disjunctions — none may ever be emitted.
        assert!(!re.contains('|'));
        let matcher = regex::Regex::new(&re).unwrap();
        // matches subdomain forms
        assert!(matcher.is_match("https://cdn.ads.example.com/x/y"));
        assert!(matcher.is_match("https://ads.example.com/"));
        // bare host — canonicalized by WebKit to a trailing slash
        assert!(matcher.is_match("http://ads.example.com/"));
        // port + query forms
        assert!(matcher.is_match("https://ads.example.com:8080/a"));
        assert!(matcher.is_match("https://ads.example.com/?q=1"));
        // must NOT match reverse-host tricks
        assert!(!matcher.is_match("https://ads.example.com.evil.io/a"));
        assert!(!matcher.is_match("https://example.com/"));
    }

    #[test]
    fn host_anchor_with_path() {
        let re = pattern_to_regex("||tracker.net/pixel.js^").unwrap();
        assert!(!re.contains('|'));
        let matcher = regex::Regex::new(&re).unwrap();
        assert!(matcher.is_match("https://tracker.net/pixel.js"));
        assert!(matcher.is_match("https://tracker.net/pixel.js?v=2"));
        assert!(matcher.is_match("https://a.tracker.net/pixel.js"));
        assert!(!matcher.is_match("https://tracker.net/other.js"));
        assert!(!matcher.is_match("https://tracker.net.evil.io/pixel.js"));
        assert!(!matcher.is_match("https://notracker.net/pixel.js"));
    }

    #[test]
    fn every_converted_rule_is_webkit_safe() {
        // The whole seed corpus (both bundled lists) must convert without
        // a single disjunction — one bad regex fails the entire store.
        let rules: Vec<String> = crate::lists::SEED_EASYLIST
            .lines()
            .chain(crate::lists::SEED_EASYPRIVACY.lines())
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let json = to_content_blocker(&rules, 120_000).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&json).unwrap();
        assert!(!parsed.is_empty(), "converter produced no rules");
        for rule in &parsed {
            let uf = rule["trigger"]["url-filter"].as_str().unwrap();
            assert!(!uf.contains('|'), "disjunction in url-filter: {uf}");
            assert!(!uf.contains("(?="), "lookahead in url-filter: {uf}");
        }
    }

    #[test]
    fn exception_rule() {
        let v = convert_rule("@@||good.com^$document").unwrap();
        assert_eq!(v["action"]["type"], "ignore-previous-rules");
    }

    #[test]
    fn third_party_resource_type() {
        let v = convert_rule("||trk.net^$script,third-party").unwrap();
        assert_eq!(v["trigger"]["resource-type"], serde_json::json!(["script"]));
        assert_eq!(v["trigger"]["load-type"], serde_json::json!(["third-party"]));
    }

    #[test]
    fn unknown_option_dropped() {
        assert!(convert_rule("||x.com^$csp=frame-ancestors").is_none());
        assert!(convert_rule("||x.com^$badfilter").is_none());
    }

    #[test]
    fn plain_substring() {
        let re = pattern_to_regex("/ads/banner.").unwrap();
        let matcher = regex::Regex::new(&re).unwrap();
        assert!(matcher.is_match("https://site.com/ads/banner.js"));
        assert!(!matcher.is_match("https://site.com/content/page.js"));
    }
}
