//! Exact-worktree trust for explicitly authorized interactive auto tasks.
//! The lock serializes Ahu writers only. Native applications do not participate;
//! rereading before replacement detects some races, not an atomic transaction.
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use crate::util::{Error, Result};

fn refused() -> Error {
    Error::new(
        "native task trust refused: unsafe, changed, unsupported, or malformed state; native configuration contents are withheld",
    )
}

fn checked(path: &Path, directory: bool) -> Result<Option<fs::Metadata>> {
    // SAFETY: geteuid takes no pointers and cannot invalidate filesystem handles.
    let owner = unsafe { libc::geteuid() };
    match fs::symlink_metadata(path) {
        Ok(m)
            if !m.file_type().is_symlink()
                && m.uid() == owner
                && m.mode() & 0o022 == 0
                && (if directory {
                    m.is_dir()
                } else {
                    m.is_file() && m.nlink() == 1
                }) =>
        {
            Ok(Some(m))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => Err(refused()),
    }
}

// Lock the existing native-home directory so all repositories serialize Ahu
// writers without creating an ahu-owned file in the user's home directory.
// Native applications do not participate in this advisory lock.
fn lock_native(path: &Path) -> Result<fs::File> {
    use std::os::fd::AsRawFd;
    checked(path, true)?.ok_or_else(refused)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| refused())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        // SAFETY: file owns this live descriptor; flock changes only its advisory lock.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(file);
        }
        let error = std::io::Error::last_os_error();
        if !matches!(error.raw_os_error(), Some(libc::EWOULDBLOCK))
            || std::time::Instant::now() >= deadline
        {
            return Err(refused());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

pub(crate) fn update(
    home: &Path,
    dir: &Path,
    _harness: &str,
    _worktree: &Path,
    _remove: bool,
) -> Result<()> {
    // Validate every component before creating or reading state. No native
    // document is copied into task state, diagnostics, or a backup file.
    checked(home, true)?.ok_or_else(refused)?;
    crate::files::check_components(dir)?;
    checked(dir, true)?.ok_or_else(refused)?;
    let _lock = lock_native(home)?;
    Err(refused())
}

// Codex 0.163.0's TUI config/read lookup prefers a saved exact cwd entry,
// even for linked worktrees whose fallback trust target is the main checkout.
// Edit the native TOML in place without serializing unrelated settings anew.
