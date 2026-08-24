pub mod resolve;
pub mod schema;

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr};
pub use schema::Config;

/// Loads the config from `path`, creating a default empty file if none
/// exists yet. Parse errors are surfaced rather than silently falling back
/// to an empty config, since that could look like data loss to the user.
pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        let config = Config::default();
        save(path, &config)?;
        return Ok(config);
    }

    let contents = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("reading config file {}", path.display()))?;
    toml::from_str(&contents).wrap_err_with(|| format!("parsing config file {}", path.display()))
}

/// Writes `config` atomically: serialize to a temp file in the same
/// directory, then rename over the target, so a crash mid-write never
/// leaves a truncated config file.
pub fn save(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }

    let serialized = toml::to_string_pretty(config)?;
    let tmp_path = tmp_path_for(path);
    std::fs::write(&tmp_path, serialized)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    tmp.set_extension("toml.tmp");
    tmp
}
