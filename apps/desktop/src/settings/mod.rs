use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
mod view;
pub(crate) use view::{PreferencesChanged, Settings, apply_appearance};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub(crate) appearance: Appearance,
    pub(crate) scale: u16,
    pub(crate) pi_path: String,
    pub(crate) shell_path: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            scale: 100,
            pi_path: String::new(),
            shell_path: String::new(),
        }
    }
}

impl Preferences {
    pub(crate) fn pi(&self) -> Result<PathBuf> {
        executable(if self.pi_path.trim().is_empty() {
            "pi"
        } else {
            self.pi_path.trim()
        })
    }
    pub(crate) fn shell(&self) -> Result<PathBuf> {
        let default = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        executable(if self.shell_path.trim().is_empty() {
            &default
        } else {
            self.shell_path.trim()
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if !matches!(self.scale, 100 | 125 | 150) {
            bail!("Choose 100%, 125% or 150%.");
        }
        if !self.pi_path.trim().is_empty() {
            self.pi()?;
        }
        self.shell()?;
        Ok(())
    }
}

pub(crate) fn executable(name: &str) -> Result<PathBuf> {
    let candidates = if Path::new(name).is_absolute() {
        vec![PathBuf::from(name)]
    } else {
        let mut paths = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .collect::<Vec<_>>();
        paths.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ]);
        paths.into_iter().map(|path| path.join(name)).collect()
    };
    use std::os::unix::fs::PermissionsExt;
    for path in candidates {
        if path
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        {
            return Ok(path);
        }
    }
    bail!("Executable not found: {name}")
}

pub(crate) fn dependency_status(preferences: &Preferences) -> String {
    let pi = preferences
        .pi()
        .and_then(|path| Ok((Command::new(&path).arg("--version").output()?, path)));
    match pi {
        Ok((output, path)) if output.status.success() => {
            format!(
                "Pi {} · {}",
                String::from_utf8_lossy(&output.stdout).trim(),
                path.display()
            )
        }
        Ok(_) => "Pi could not start. Check its configuration.".into(),
        Err(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_path_does_not_replace_saved_preferences() {
        let saved = Preferences::default();
        let candidate = Preferences {
            shell_path: "/does/not/exist".into(),
            ..saved.clone()
        };
        assert!(candidate.validate().is_err());
        assert!(saved.shell_path.is_empty());
    }
}
