use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Get the cache directory path
pub fn cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("Failed to get HOME environment variable")?;
    Ok(PathBuf::from(home).join(".capsule-vm/cache"))
}

/// Ensure cache directory exists
pub fn ensure_cache_dir() -> Result<PathBuf> {
    let dir = cache_dir()?;
    fs::create_dir_all(&dir).context("Failed to create cache directory")?;
    Ok(dir)
}

/// Download a file to cache if it doesn't exist, return path
pub fn ensure_file(url: &str, filename: &str) -> Result<PathBuf> {
    let cache = ensure_cache_dir()?;
    let path = cache.join(filename);

    if path.exists() {
        println!("Using cached {}", filename);
        return Ok(path);
    }

    println!("Downloading {} to cache...", filename);
    download_file(url, &path)?;
    println!("Cached {}", filename);

    Ok(path)
}

/// Download a file from URL to destination
fn download_file(url: &str, dest: &Path) -> Result<()> {
    use std::process::Command;

    // Simple implementation using curl
    let status = Command::new("curl")
        .arg("-fsSL")
        .arg(url)
        .arg("-o")
        .arg(dest)
        .status()
        .context("Failed to run curl")?;

    if !status.success() {
        anyhow::bail!("Failed to download {}", url);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_dir() {
        let dir = cache_dir().unwrap();
        assert!(dir.to_string_lossy().contains(".capsule-vm/cache"));
    }
}
