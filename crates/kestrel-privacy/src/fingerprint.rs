//! Fingerprint-randomization scripts injected at document-start (MAIN world).
//!
//! Modes:
//! - Standard: canvas / WebGL / audio noise + hardware spoofs
//! - Strict: adds font-metric and ClientRect jitter
//! - Off: nothing injected

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerprintMode {
    Off,
    Standard,
    Strict,
}

impl FingerprintMode {
    pub fn from_key(s: &str) -> Self {
        match s {
            "strict" => FingerprintMode::Strict,
            "off" => FingerprintMode::Off,
            _ => FingerprintMode::Standard,
        }
    }
    pub fn key(&self) -> &'static str {
        match self {
            FingerprintMode::Off => "off",
            FingerprintMode::Standard => "standard",
            FingerprintMode::Strict => "strict",
        }
    }
    pub fn script(&self) -> &'static str {
        match self {
            FingerprintMode::Off => "",
            _ => include_str!("../assets/fingerprint/shield.js"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_present_and_safe() {
        let s = FingerprintMode::Standard.script();
        assert!(s.contains("__kestrelShield"));
        assert!(s.len() > 1000);
        assert_eq!(FingerprintMode::Off.script(), "");
    }
}
