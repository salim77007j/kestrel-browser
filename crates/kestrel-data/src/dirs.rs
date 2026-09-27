//! XDG-aware paths. All Kestrel state lives under the user's config/data/cache
//! directories so it integrates cleanly with backups and distro conventions.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn base_config() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".config"))
        .join("kestrel")
}

pub fn base_data() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".local/share"))
        .join("kestrel")
}

pub fn base_cache() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache"))
        .join("kestrel")
}

pub fn default_downloads() -> PathBuf {
    std::env::var("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join("Downloads"))
}

pub fn ensure_dirs() -> std::io::Result<()> {
    for d in [base_config(), base_data(), base_cache(), base_data().join("filters/compiled")] {
        std::fs::create_dir_all(d)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn paths_are_under_home() {
        assert!(super::base_config().starts_with(std::env::var("HOME").unwrap()));
        assert!(super::base_data().ends_with("kestrel"));
    }
}
