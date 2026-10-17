use super::HistoryResult;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    path::{Component, Path, PathBuf},
};

pub fn resolve_directory(
    enabled: bool,
    explicit: Option<PathBuf>,
    home: Option<PathBuf>,
    xdg: Option<PathBuf>,
    mac: bool,
) -> HistoryResult<Option<PathBuf>> {
    if !enabled {
        return Ok(None);
    }
    if let Some(p) = explicit {
        if !p.is_absolute() {
            return Err("--history-dir must be absolute".into());
        }
        return Ok(Some(p));
    }
    if !mac && let Some(p) = xdg.filter(|p| p.is_absolute()) {
        return Ok(Some(p.join("monkeytype-tui")));
    }
    let home = home
        .filter(|p| p.is_absolute())
        .ok_or("history needs an absolute home or explicit directory")?;
    Ok(Some(home.join(if mac {
        "Library/Application Support/monkeytype-tui"
    } else {
        ".local/state/monkeytype-tui"
    })))
}

pub(super) fn private_directory(path: &Path, create: bool) -> HistoryResult<()> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("history path must be absolute without parent traversal".into());
    }
    // Parent locations may contain OS aliases such as macOS /tmp -> /private/tmp.
    // The app directory itself and database/lock files must never be links.
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Ok(_) => return Err("history directory must not be a symbolic link".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
            let mut d = fs::DirBuilder::new();
            d.recursive(true);
            #[cfg(unix)]
            d.mode(0o700);
            d.create(path)?;
            if fs::symlink_metadata(path)?.file_type().is_symlink() {
                return Err("history directory is a symbolic link".into());
            }
        }
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err("history directory must be private (0700)".into());
    }
    Ok(())
}
pub(super) fn regular_or_missing(path: &Path) -> HistoryResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("history file is not a regular file".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn private_file(path: &Path) -> HistoryResult<File> {
    regular_or_missing(path)?;
    let mut o = OpenOptions::new();
    o.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        o.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = o.open(path)?;
    #[cfg(unix)]
    if file.metadata()?.permissions().mode() & 0o077 != 0
        || file.metadata()?.permissions().mode() & 0o200 == 0
    {
        return Err("history file must be private and writable (0600)".into());
    }
    Ok(file)
}
