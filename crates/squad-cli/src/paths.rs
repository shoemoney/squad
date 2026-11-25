use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub enum Platform {
    Linux,
    MacOs,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

fn home(env: &impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    env("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME must contain a nonempty path.".into())
}

fn xdg(env: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    env(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

pub fn config_dir(
    platform: Platform,
    env: &impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let base = match platform {
        Platform::Linux => match xdg(env, "XDG_CONFIG_HOME") {
            Some(path) => path,
            None => home(env)?.join(".config"),
        },
        Platform::MacOs => home(env)?.join("Library/Application Support"),
    };
    Ok(base.join("squad"))
}

// TODO(M0 merge): use SquadConfig. No environment file is read here.
// The selected file only supplies the base for relative environment paths.
pub fn data_dir(
    override_path: Option<&Path>,
    selected_file: Option<&Path>,
    cwd: &Path,
    platform: Platform,
    env: &impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        if path.as_os_str().is_empty() {
            return Err("--data-dir must contain a nonempty path.".into());
        }
        return Ok(cwd.join(path));
    }
    if let Some(value) = env("SQUAD_DATA_DIR") {
        if value.is_empty() {
            return Err("SQUAD_DATA_DIR must contain a nonempty path.".into());
        }
        let base = selected_file
            .map(|p| cwd.join(p))
            .and_then(|p| p.parent().map(Path::to_owned))
            .unwrap_or_else(|| cwd.to_owned());
        return Ok(base.join(value));
    }
    let base = match platform {
        Platform::Linux => match xdg(env, "XDG_DATA_HOME") {
            Some(path) => path,
            None => home(env)?.join(".local/share"),
        },
        Platform::MacOs => home(env)?.join("Library/Application Support"),
    };
    Ok(base.join("squad"))
}

/// Create new components privately, and secure the application directory itself.
/// Existing parent directories (such as HOME or XDG_DATA_HOME) are not changed.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other(
            "application directory must be a directory, not a symlink",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn check_writable(path: &Path) -> io::Result<()> {
    use std::io::Write;
    create_private_dir(path)?;
    // Exclusive, private probe; never touches room rows or an existing file.
    let probe = path.join(format!(".squad-write-check-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&probe)?;
    let result = file
        .write_all(b"squad doctor\n")
        .and_then(|()| file.sync_all());
    drop(file);
    let cleanup = fs::remove_file(probe);
    result.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env(name: &str) -> Option<OsString> {
        match name {
            "HOME" => Some("/home/test".into()),
            _ => None,
        }
    }
    #[test]
    fn defaults_for_both_platforms() {
        let cwd = Path::new("/work");
        assert_eq!(
            data_dir(None, None, cwd, Platform::Linux, &env).unwrap(),
            Path::new("/home/test/.local/share/squad")
        );
        assert_eq!(
            config_dir(Platform::Linux, &env).unwrap(),
            Path::new("/home/test/.config/squad")
        );
        assert_eq!(
            data_dir(None, None, cwd, Platform::MacOs, &env).unwrap(),
            Path::new("/home/test/Library/Application Support/squad")
        );
        assert_eq!(
            config_dir(Platform::MacOs, &env).unwrap(),
            Path::new("/home/test/Library/Application Support/squad")
        );
    }
    #[test]
    fn precedence_and_relative_bases() {
        let injected = |name: &str| match name {
            "SQUAD_DATA_DIR" => Some("relative".into()),
            "XDG_DATA_HOME" => Some("/xdg/data".into()),
            "XDG_CONFIG_HOME" => Some("/xdg/config".into()),
            _ => env(name),
        };
        let resolve = |arg, file| {
            data_dir(arg, file, Path::new("/work"), Platform::Linux, &injected).unwrap()
        };
        assert_eq!(
            resolve(Some(Path::new("flag")), Some(Path::new("/config/.env"))),
            Path::new("/work/flag")
        );
        assert_eq!(
            resolve(None, Some(Path::new("/config/.env"))),
            Path::new("/config/relative")
        );
        assert_eq!(resolve(None, None), Path::new("/work/relative"));
        assert_eq!(
            config_dir(Platform::Linux, &injected).unwrap(),
            Path::new("/xdg/config/squad")
        );
        let xdg_env = |name: &str| {
            if name == "XDG_DATA_HOME" {
                Some("/xdg/data".into())
            } else {
                env(name)
            }
        };
        assert_eq!(
            data_dir(None, None, Path::new("/work"), Platform::Linux, &xdg_env).unwrap(),
            Path::new("/xdg/data/squad")
        );
        let relative_xdg = |name: &str| {
            if name.starts_with("XDG_") {
                Some("relative".into())
            } else {
                env(name)
            }
        };
        assert_eq!(
            config_dir(Platform::Linux, &relative_xdg).unwrap(),
            Path::new("/home/test/.config/squad")
        );
        assert!(
            data_dir(None, None, Path::new("/work"), Platform::Linux, &|_| Some(
                OsString::new()
            ))
            .is_err()
        );
    }
    #[test]
    fn private_creation_and_writable_probe() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("nested/squad");
        check_writable(&dir).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(dir.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}
