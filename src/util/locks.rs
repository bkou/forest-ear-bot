//! Telling a locked file apart from one we genuinely may not touch.
//!
//! On Windows a file another process holds open without `FILE_SHARE_DELETE`
//! cannot be unlinked, and the refusal arrives as `Access is denied. (os error
//! 5)` — byte for byte the error a permissions problem gives. The two need
//! opposite answers from whoever is reading the Discord reply (stop the server
//! vs. fix an ACL), so this module separates them.
//!
//! Every probe here runs synchronously, inside the failure path. A lock lives
//! and dies with the process holding it: the same check run afterwards in a
//! separate shell, with the server stopped, reports "unlocked" and is worse than
//! no check at all.

use std::path::{Path, PathBuf};
use sysinfo::{ProcessExt, ProcessRefreshKind, RefreshKind, System, SystemExt};

/// Another handle is open on the file. This is the lock signature.
#[cfg(windows)]
const ERROR_SHARING_VIOLATION: i32 = 32;
#[cfg(windows)]
const ERROR_LOCK_VIOLATION: i32 = 33;

/// Give up walking a directory after this many entries. A save folder that big
/// is pathological, and the walk happens while someone waits on a reply.
const MAX_PROBE: usize = 4096;

/// How many locked paths are worth naming before the list stops being useful.
const MAX_REPORTED: usize = 5;

/// Keep the whole message inside a Discord reply, which the error handler still
/// has to prefix. A diagnosis that fails to send diagnoses nothing.
const MAX_MESSAGE: usize = 1800;

/// Open `path` for reading while denying every other kind of access, so the call
/// fails if anyone else already has it open.
#[cfg(windows)]
fn exclusive_open(path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .map(drop)
}

/// True only when the open was refused *because someone else has the file* —
/// not when it was refused because we may not read it, which is the very thing
/// this is trying to rule out.
#[cfg(windows)]
fn is_locked(path: &Path) -> bool {
    matches!(
        exclusive_open(path).err().and_then(|e| e.raw_os_error()),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}

#[cfg(not(windows))]
fn is_locked(_path: &Path) -> bool {
    false
}

/// Files at or under `target` that another process is holding open, up to
/// `limit` of them.
///
/// Only files are probed. A directory handle is held by anything merely looking
/// at the folder — Explorer, a shell sitting in it — so probing directories
/// themselves reports "locks" that would not have blocked the delete.
pub fn locked_paths(target: &Path, limit: usize) -> Vec<PathBuf> {
    // Unlinking on POSIX never fails because a file is open, so there is nothing
    // here to find and no reason to pay for the walk.
    if cfg!(not(windows)) || limit == 0 {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut stack = vec![target.to_path_buf()];
    let mut seen = 0usize;

    while let Some(path) = stack.pop() {
        seen += 1;
        if seen > MAX_PROBE {
            break;
        }

        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            stack.extend(entries.flatten().map(|entry| entry.path()));
            continue;
        }

        if is_locked(&path) {
            found.push(path);
            if found.len() >= limit {
                break;
            }
        }
    }

    found
}

/// The first file at or under `target` that another process holds open.
///
/// Cheap enough to run before doing anything expensive, since it stops at the
/// first hit.
pub fn first_locked(target: &Path) -> Option<PathBuf> {
    locked_paths(target, 1).into_iter().next()
}

/// A running process whose executable lives under `dir`, if there is one.
///
/// Windows names no culprit in the error and finding the real one needs the
/// Restart Manager API. This is the cheap approximation that answers the only
/// question anybody actually has: is the game server up? Bounding the search to
/// the game's own folder is what keeps it from blaming an unrelated program that
/// happens to run from the same drive.
pub fn process_under(dir: &Path) -> Option<String> {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    sys.processes()
        .values()
        .find(|process| process.exe().starts_with(dir))
        .map(|process| format!("{} (pid {})", process.name(), process.pid()))
}

/// What happened when the path was renamed out of the way and back.
enum Rename {
    /// Renamed and restored — nothing is holding the name.
    Allowed,
    /// Refused outright.
    Refused(std::io::Error),
    /// Renamed, but could not be put back. The path now lives elsewhere.
    Stranded(PathBuf),
}

/// Rename `path` aside and immediately back, to see whether the directory entry
/// itself can be touched.
///
/// This is the discriminator: a lock refuses a rename, while the ACL oddities
/// that block a delete usually still permit one. The name is only borrowed for
/// the length of the second call, and a failure to restore it is reported rather
/// than swallowed.
fn probe_rename(path: &Path) -> Rename {
    let Some(name) = path.file_name() else {
        return Rename::Allowed;
    };

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let probe = path.with_file_name(format!(
        "{}.forest-ear-probe-{}",
        name.to_string_lossy(),
        stamp
    ));

    if let Err(e) = std::fs::rename(path, &probe) {
        return Rename::Refused(e);
    }
    match std::fs::rename(&probe, path) {
        Ok(()) => Rename::Allowed,
        Err(_) => Rename::Stranded(probe),
    }
}

/// The state of the path as the filesystem reports it right now.
///
/// `symlink_metadata` so this describes the thing being deleted rather than
/// whatever it points at.
fn describe(path: &Path) -> String {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) => return format!("no metadata ({})", e),
    };

    let kind = if metadata.file_type().is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "folder"
    } else {
        "file"
    };

    let mut out = format!("{}, {} bytes", kind, metadata.len());
    if metadata.permissions().readonly() {
        out.push_str(", read-only");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        out.push_str(&format!(
            ", attributes 0x{:08x}",
            metadata.file_attributes()
        ));
    }
    out
}

/// Render a locked path against the target it was found under, so a folder's
/// report reads as a list of names rather than of near-identical absolute paths.
fn relative(path: &Path, target: &Path) -> String {
    path.strip_prefix(target)
        .or_else(|_| path.strip_prefix(target.parent().unwrap_or(target)))
        .map(|short| short.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// Explain a failed delete, probing the filesystem as it stands *now*.
///
/// `game_dir` bounds the search for a process to blame; without it the verdict
/// still holds, it just names nobody.
pub fn diagnose(target: &Path, err: &std::io::Error, game_dir: Option<&Path>) -> String {
    let locked = locked_paths(target, MAX_REPORTED);

    let mut out = format!("Could not delete {}: {}", target.display(), err);

    if locked.is_empty() {
        // Worth disturbing the path only here: this is the case where a lock and
        // a permissions problem are still indistinguishable from the error alone.
        match probe_rename(target) {
            Rename::Allowed => out.push_str(
                "\n\nNothing has it open and renaming it works, so neither a lock nor the \
                 folder's permissions explain this. Suspect something that blocks deletes \
                 specifically — antivirus, or Windows' controlled folder access.",
            ),
            Rename::Refused(e) => out.push_str(&format!(
                "\n\nNothing has it open, and renaming it is refused too ({}). That points \
                 at permissions on the file or its parent folder, not at the game server.",
                e
            )),
            Rename::Stranded(probe) => out.push_str(&format!(
                "\n\n**A diagnostic rename could not be undone: the path is now {} and needs \
                 moving back to {} by hand.**",
                probe.display(),
                target.display()
            )),
        }
    } else {
        let count = if locked.len() == 1 {
            "it".to_string()
        } else {
            format!(
                "{}{} files under it",
                if locked.len() == MAX_REPORTED {
                    "at least "
                } else {
                    ""
                },
                locked.len()
            )
        };
        out.push_str(&format!(
            "\n\nAnother process is holding {} open without permitting deletes, which is what \
             Windows reports as \"Access is denied\". This is a lock, not a permissions problem.\
             \n\nHeld open:",
            count
        ));
        for path in &locked {
            out.push_str(&format!("\n- `{}`", relative(path, target)));
        }

        match game_dir.and_then(process_under) {
            Some(process) => out.push_str(&format!(
                "\n\n`{}` is running out of the game folder and is the likely holder. \
                 Stop it and try again.",
                process
            )),
            None => out.push_str(
                "\n\nNo process is running out of the game folder, so the holder is something \
                 else — check what has the save open before retrying.",
            ),
        }
    }

    out.push_str(&format!(
        "\n\nDetails: os error {}; {}",
        err.raw_os_error()
            .map_or_else(|| "none".to_string(), |code| code.to_string()),
        describe(target)
    ));

    if out.chars().count() > MAX_MESSAGE {
        // Count in chars, not bytes, so this can't split a multi-byte character.
        out = out.chars().take(MAX_MESSAGE - 1).chain(['…']).collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("forest_bot_locks_{}_{}", stamp, name));
        std::fs::write(&path, b"contents").unwrap();
        path
    }

    /// The rename probe borrows the name and must give it back — a diagnostic
    /// that moves the user's save is worse than the failure it explains.
    #[test]
    fn rename_probe_restores_the_path() {
        let path = temp_file("rename.sav");

        assert!(matches!(probe_rename(&path), Rename::Allowed));
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), b"contents");

        // And nothing is left lying beside it.
        let parent = path.parent().unwrap();
        let strays = std::fs::read_dir(parent)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("forest-ear-probe"))
            .count();
        assert_eq!(strays, 0);

        std::fs::remove_file(&path).unwrap();
    }

    /// Whatever the verdict, the raw error has to survive into the message: it
    /// is the one piece nobody can re-derive after the fact.
    #[test]
    fn diagnosis_keeps_the_raw_error() {
        let path = temp_file("diagnose.sav");
        let err = std::io::Error::from_raw_os_error(5);

        let report = diagnose(&path, &err, None);
        assert!(
            report.contains("os error 5"),
            "unexpected report: {}",
            report
        );
        assert!(
            report.contains(&path.display().to_string()),
            "should name the path: {}",
            report
        );
        // The probe ran against a real file, so it is still there afterwards.
        assert!(path.exists());

        std::fs::remove_file(&path).unwrap();
    }

    /// A missing file has no metadata, and the diagnosis has to say so rather
    /// than panic on the way to explaining a failure.
    #[test]
    fn diagnosis_survives_a_vanished_path() {
        let path = std::env::temp_dir().join("forest_bot_locks_definitely_not_here");
        let _ = std::fs::remove_file(&path);

        let report = diagnose(&path, &std::io::Error::from_raw_os_error(5), None);
        assert!(
            report.contains("no metadata"),
            "unexpected report: {}",
            report
        );
    }

    /// Nothing is locked on this platform's terms, and asking must not walk off
    /// into an error.
    #[test]
    fn locked_paths_is_bounded_and_safe() {
        let path = temp_file("probe.sav");
        assert!(locked_paths(&path, 0).is_empty());
        // On Windows an unheld file probes clean; elsewhere the walk is skipped.
        assert!(first_locked(&path).is_none());
        std::fs::remove_file(&path).unwrap();
    }
}
