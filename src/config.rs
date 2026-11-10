use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const APP_NAME: &str = "LGQuickSettingsPatcher";
const DEFAULT_CONFIG: &str = r#"# This file stays in your user config directory, outside the Git repository.
# ssh_target may be root@TV_IP or an OpenSSH alias (including ProxyJump settings).
# ssh_config_file optionally selects a custom OpenSSH config; blank uses OpenSSH defaults.
# Authentication uses OpenSSH keys/agent or a one-time system prompt; do not put passwords here.
ssh_target = ""
model = ""
firmware_label = ""
identity_file = ""
ssh_config_file = ""
port = 0
"#;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct UserConfig {
    pub ssh_target: String,
    pub ssh_config_file: String,
    pub model: String,
    pub firmware_label: String,
    pub identity_file: String,
    pub port: u16,
    #[serde(flatten)]
    pub(crate) extra: std::collections::BTreeMap<String, toml::Value>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot determine the user configuration directory")]
    NoConfigDirectory,
    #[error("cannot access connection settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid connection settings: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("cannot serialize connection settings: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("invalid SSH target: use an OpenSSH alias or root@host without whitespace")]
    InvalidTarget,
    #[error("invalid {0}; use at most 160 printable characters")]
    InvalidField(&'static str),
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    let base = dirs::config_dir().ok_or(ConfigError::NoConfigDirectory)?;
    Ok(base.join(APP_NAME).join("config.toml"))
}

fn config_directory(path: &Path) -> Result<(&Path, bool), ConfigError> {
    let directory = path.parent().ok_or(ConfigError::NoConfigDirectory)?;
    if directory.as_os_str().is_empty() || directory == Path::new(".") {
        Ok((Path::new("."), false))
    } else if directory == Path::new("/") {
        Ok((directory, false))
    } else {
        Ok((directory, true))
    }
}
fn prepare_directory(directory: &Path, secure: bool) -> Result<(), ConfigError> {
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    if secure {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    let _ = secure;
    Ok(())
}

pub(crate) fn ensure_private_directory(directory: &Path) -> Result<(), ConfigError> {
    prepare_directory(directory, true)
}

fn ensure_config_directory(path: &Path, secure_parent: bool) -> Result<&Path, ConfigError> {
    let (directory, can_secure_parent) = config_directory(path)?;
    prepare_directory(directory, secure_parent && can_secure_parent)?;
    Ok(directory)
}

pub fn load_or_create() -> Result<(PathBuf, UserConfig), ConfigError> {
    let path = config_path()?;
    let config = load_or_create_with_policy(&path, true)?;
    Ok((path, config))
}

pub fn load_or_create_at(path: &Path) -> Result<UserConfig, ConfigError> {
    load_or_create_with_policy(path, false)
}

fn load_or_create_with_policy(path: &Path, secure_parent: bool) -> Result<UserConfig, ConfigError> {
    ensure_config_directory(path, secure_parent)?;
    if !path.exists() {
        fs::write(path, DEFAULT_CONFIG)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
    }
    let text = fs::read_to_string(path)?;
    let config: UserConfig = toml::from_str(&text)?;
    validate(&config)?;
    Ok(config)
}

pub fn save(path: &Path, config: &UserConfig) -> Result<(), ConfigError> {
    save_with_policy(path, config, true)
}

pub fn save_at(path: &Path, config: &UserConfig) -> Result<(), ConfigError> {
    save_with_policy(path, config, false)
}

fn save_with_policy(
    path: &Path,
    config: &UserConfig,
    secure_parent: bool,
) -> Result<(), ConfigError> {
    validate(config)?;
    let contents = toml::to_string_pretty(config)?;
    let directory = ensure_config_directory(path, secure_parent)?;
    let file_name = path
        .file_name()
        .ok_or(ConfigError::NoConfigDirectory)?
        .to_string_lossy();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary_path =
        directory.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary_path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result?;
    Ok(())
}

pub fn validate(config: &UserConfig) -> Result<(), ConfigError> {
    if !config.ssh_target.is_empty()
        && (config.ssh_target.starts_with('-')
            || config.ssh_target.chars().any(|character| {
                !(character.is_ascii_alphanumeric()
                    || matches!(character, '@' | '.' | '_' | ':' | '-' | '[' | ']'))
            }))
    {
        return Err(ConfigError::InvalidTarget);
    }
    for (name, value) in [
        ("model", config.model.as_str()),
        ("firmware label", config.firmware_label.as_str()),
        ("identity path", config.identity_file.as_str()),
        ("OpenSSH config path", config.ssh_config_file.as_str()),
    ] {
        if value.len() > 160 || value.chars().any(char::is_control) {
            return Err(ConfigError::InvalidField(name));
        }
    }
    Ok(())
}

pub fn identity_path(config: &UserConfig) -> Option<PathBuf> {
    expand_user_path(&config.identity_file)
}

pub fn ssh_config_path(config: &UserConfig) -> Option<PathBuf> {
    expand_user_path(&config.ssh_config_file)
}

fn expand_user_path(value: &str) -> Option<PathBuf> {
    if value.is_empty() {
        None
    } else if value == "~" {
        dirs::home_dir()
    } else if let Some(rest) = value.strip_prefix("~/") {
        dirs::home_dir().map(|home| home.join(rest))
    } else {
        Some(PathBuf::from(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("config-save-test-{}-{name}", std::process::id()))
            .join("config.toml")
    }

    #[test]
    fn saved_settings_round_trip_without_dropping_unknown_keys() {
        let path = test_path("round-trip");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut config = UserConfig::default();
        config.ssh_target = "root@tv.local".into();
        config.model = "OLED55".into();
        config.firmware_label = "webOS 24".into();
        config.identity_file = "~/.ssh/tv".into();
        config.ssh_config_file = "~/.ssh/jump_config".into();
        config.port = 2222;
        config.extra.insert(
            "future_setting".into(),
            toml::Value::String("preserved".into()),
        );

        save(&path, &config).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let loaded: UserConfig = toml::from_str(&text).unwrap();
        assert_eq!(loaded.ssh_target, "root@tv.local");
        assert_eq!(loaded.model, "OLED55");
        assert_eq!(loaded.firmware_label, "webOS 24");
        assert_eq!(loaded.identity_file, "~/.ssh/tv");
        assert_eq!(loaded.ssh_config_file, "~/.ssh/jump_config");
        assert_eq!(loaded.port, 2222);
        assert_eq!(
            loaded.extra.get("future_setting"),
            Some(&toml::Value::String("preserved".into()))
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn custom_config_paths_preserve_parent_permissions() {
        let path = test_path("custom-parent");
        let directory = path.parent().unwrap();
        fs::create_dir_all(directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut config = load_or_create_at(&path).unwrap();
        config.ssh_config_file = "relative/ssh_config".into();
        save_at(&path, &config).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(directory).unwrap().permissions().mode() & 0o777,
                0o755
            );
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            load_or_create_at(&path).unwrap().ssh_config_file,
            "relative/ssh_config"
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ssh_config_path_expands_tilde_and_keeps_relative_paths() {
        let mut config = UserConfig::default();
        config.ssh_config_file = "custom/ssh_config".into();
        assert_eq!(
            ssh_config_path(&config),
            Some(PathBuf::from("custom/ssh_config"))
        );
        config.ssh_config_file = "~/.ssh/config".into();
        assert_eq!(
            ssh_config_path(&config),
            dirs::home_dir().map(|home| home.join(".ssh/config"))
        );
    }

    #[test]
    fn older_settings_without_ssh_config_use_the_system_default() {
        let config: UserConfig = toml::from_str("ssh_target = \"lg-tv\"\n").unwrap();
        assert!(config.ssh_config_file.is_empty());
        assert_eq!(ssh_config_path(&config), None);
    }

    #[test]
    fn invalid_settings_do_not_replace_saved_file() {
        let path = test_path("invalid");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "keep this file intact").unwrap();
        let mut config = UserConfig::default();
        config.ssh_target = "-oProxyCommand=unsafe".into();

        assert!(matches!(
            save(&path, &config),
            Err(ConfigError::InvalidTarget)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"keep this file intact");

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
