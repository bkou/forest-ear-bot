//! Shared logic for the generic save backup/restore commands.
//!
//! Paths here are user-facing, not filesystem paths: in `Palworld_Server/Saved`
//! the first segment names an entry in the saves root (a `.lnk`, a symlink, or a
//! plain directory) and the rest descends into that entry's target.

use directories::UserDirs;
use lnk::ShellLink;
use poise::serenity_prelude as serenity;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::{Context, Error};

/// Footer marker identifying a message this bot posted as a backup. Restore
/// refuses any message without it, so it can't be pointed at arbitrary zips.
pub const BACKUP_MARKER: &str = "forest-ear-bot:backup:v1";

/// The folder holding one entry per game. `GAME_SAVES_ROOT` overrides it.
///
/// Errors if the folder is missing, so a bad setup reports itself plainly
/// instead of surfacing later as "no entry named X".
pub fn saves_root() -> Result<PathBuf, Error> {
    let (root, from_env) = match std::env::var("GAME_SAVES_ROOT") {
        Ok(root) => (PathBuf::from(root), true),
        Err(_) => {
            let user_dirs = UserDirs::new().ok_or("Could not locate the user directories")?;
            let desktop = user_dirs
                .desktop_dir()
                .ok_or("Could not locate the desktop directory")?;
            (desktop.join("saves"), false)
        }
    };

    if !root.exists() {
        let hint = if from_env {
            "GAME_SAVES_ROOT points there — fix the variable or create the folder."
        } else {
            "Create it and put shortcuts to your game folders inside, or set GAME_SAVES_ROOT."
        };
        return Err(format!("Saves folder {} does not exist. {}", root.display(), hint).into());
    }
    if !root.is_dir() {
        return Err(format!("Saves folder {} is a file, not a folder.", root.display()).into());
    }

    Ok(root)
}

/// Take one step from `dir` into `segment`.
///
/// A segment may name a real folder, a symlink, or a `.lnk` shortcut (with or
/// without the extension). Because every level goes through here, shortcuts are
/// followed wherever they appear, not just at the saves root.
fn step(dir: &Path, segment: &str) -> Result<PathBuf, Error> {
    // Only plain names — no `..`, no absolute paths, no embedded separators.
    let mut parts = Path::new(segment).components();
    let part = match (parts.next(), parts.next()) {
        (Some(Component::Normal(part)), None) => part,
        _ => {
            return Err(format!(
                "`{}` is not allowed in a path — only plain folder names are.",
                segment
            )
            .into())
        }
    };

    let direct = dir.join(part);
    if direct.exists() {
        if is_lnk(&direct) {
            return resolve_lnk(&direct);
        }
        // Plain directory or symlink — canonicalize follows the link.
        return std::fs::canonicalize(&direct)
            .map_err(|e| format!("{} is unreachable: {}", direct.display(), e).into());
    }

    // The segment may name a shortcut without its extension.
    let with_lnk = dir.join(format!("{}.lnk", segment));
    if with_lnk.exists() {
        return resolve_lnk(&with_lnk);
    }

    Err(format!(
        "`{}` does not exist in {}. Use /list_saves to see what's available.",
        segment,
        dir.display()
    )
    .into())
}

/// True if `path` names a Windows shortcut file.
pub fn is_lnk(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("lnk")
}

/// Read a `.lnk` shortcut and return the directory it points at.
///
/// `lnk` 0.5 slices its buffer using offsets read straight out of the file
/// without bounds-checking them, so a shortcut it mis-parses — a network/UNC
/// target, notably — panics instead of returning an error. Contain that here, at
/// the one place shortcuts are read, so a single bad file downgrades to a
/// message naming it rather than killing the whole command.
pub fn resolve_lnk(path: &Path) -> Result<PathBuf, Error> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_lnk(path))).unwrap_or_else(|_| {
        Err(format!(
            "Shortcut {} could not be parsed (unsupported .lnk layout, e.g. a network/UNC target)",
            path.display()
        )
        .into())
    })
}

/// The actual parse. Only ever called through [`resolve_lnk`], which contains
/// panics from the underlying crate.
fn read_lnk(path: &Path) -> Result<PathBuf, Error> {
    // The codepage is only consulted for shortcuts storing non-Unicode strings;
    // Unicode ones ignore it. WINDOWS_1252 is the usual Western default.
    let link = ShellLink::open(path, lnk::encoding::WINDOWS_1252)
        .map_err(|e| format!("Could not read shortcut {}: {:?}", path.display(), e))?;

    // Builds the full target from the LinkInfo structure, handling local and
    // network paths and appending the common path suffix.
    let target = link
        .link_target()
        .ok_or_else(|| format!("Shortcut {} has no resolvable target path", path.display()))?;

    let target = PathBuf::from(target);
    std::fs::canonicalize(&target)
        .map_err(|e| format!("Shortcut target {} is unreachable: {}", target.display(), e).into())
}

/// Resolve a user-facing path (`entry/sub/folder`) to an absolute directory.
///
/// Walks it one segment at a time from the saves root, so the root entry and
/// everything below it are treated identically — a shortcut is followed at
/// whatever depth it appears.
///
/// Containment comes from each step being a plain name that must already exist
/// in the folder reached so far, so it can only land where the folder tree and
/// its shortcuts lead — never at a location of its own choosing.
pub fn resolve(path: &str) -> Result<PathBuf, Error> {
    let path = path.trim().trim_matches('/').replace('\\', "/");
    if path.is_empty() {
        return Err("Empty path. Try something like `Palworld_Server/Saved`.".into());
    }

    let mut current = saves_root()?;
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(format!("`{}` has an empty path segment", path).into());
        }
        current = step(&current, segment)?;
    }

    if !current.is_dir() {
        return Err(format!("`{}` is a file, not a folder", path).into());
    }

    Ok(current)
}

/// Recursively zip a directory's contents into an in-memory archive.
///
/// Paths inside the archive are relative to `dir`, so extracting into a folder
/// reproduces the original layout.
pub fn zip_dir(dir: &Path) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                let path = entry.path();
                let name = path
                    .strip_prefix(dir)
                    .map_err(|e| format!("{}", e))?
                    .to_string_lossy()
                    .replace('\\', "/");

                if path.is_dir() {
                    zip.add_directory(format!("{}/", name), options)?;
                    stack.push(path);
                } else {
                    zip.start_file(name, options)?;
                    let contents = std::fs::read(&path)?;
                    zip.write_all(&contents)?;
                }
            }
        }
        zip.finish()?;
    }
    Ok(buf)
}

/// Delete everything inside `dir`, leaving the directory itself in place.
pub fn wipe_dir(dir: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// True if the directory has no entries.
pub fn is_empty(dir: &Path) -> Result<bool, Error> {
    Ok(std::fs::read_dir(dir)?.next().is_none())
}

fn sanitize(path: &str) -> String {
    path.replace(['/', '\\'], "_")
}

/// A readable, filename-safe, sortable local timestamp: `20260726-143200`.
fn filename_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Discord rejects embed field values longer than this.
const MAX_FIELD: usize = 1024;

/// Clip a field value to Discord's limit — an over-long one would make the
/// entire message fail to send, losing the backup with it.
fn truncate_field(text: &str) -> String {
    if text.chars().count() <= MAX_FIELD {
        return text.to_string();
    }
    // Count in chars, not bytes, so this can't split a multi-byte character.
    text.chars().take(MAX_FIELD - 1).chain(['…']).collect()
}

/// Zip `dir` and post it as a backup message carrying `path` for later restore.
///
/// Both `/backup` and the pre-restore safety copy go through here, so every
/// backup message has the same shape and any of them can be restored.
pub async fn post_backup(
    ctx: Context<'_>,
    path: &str,
    resolved_path: &Path,
    title: &str,
    description: Option<&str>,
) -> Result<(), Error> {
    let dir_owned = resolved_path.to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || zip_dir(&dir_owned)).await??;
    let size_mb = bytes.len() as f64 / 1_048_576.0;

    let filename = format!("{}_{}.zip", sanitize(path), filename_stamp());

    let mut embed = serenity::CreateEmbed::new()
        .title(format!("{}: {}", title, path))
        .field("path", path, false)
        .field("size", format!("{:.2} MB", size_mb), true)
        .footer(serenity::CreateEmbedFooter::new(BACKUP_MARKER))
        .timestamp(serenity::Timestamp::now());

    if let Some(description) = description.map(str::trim).filter(|d| !d.is_empty()) {
        embed = embed.field("description", truncate_field(description), false);
    }

    // No size pre-check — just attempt the upload and let Discord decide. A
    // rejection here surfaces as an error, which for restore means aborting
    // before anything is wiped.
    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(serenity::CreateAttachment::bytes(bytes, filename)),
    )
    .await
    .map_err(|e| format!("Upload of `{}` ({:.1} MB) failed: {}", path, size_mb, e))?;

    Ok(())
}

/// Recover the path a backup message was created from.
///
/// Returns `None` unless the message carries this bot's marker, so restore can
/// refuse messages that merely happen to have a zip attached.
pub fn parse_backup_path(message: &serenity::Message) -> Option<String> {
    message.embeds.iter().find_map(|embed| {
        let marked = embed
            .footer
            .as_ref()
            .is_some_and(|f| f.text == BACKUP_MARKER);
        if !marked {
            return None;
        }
        embed
            .fields
            .iter()
            .find(|f| f.name == "path")
            .map(|f| f.value.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `GAME_SAVES_ROOT` is process-global but cargo runs tests in parallel, so
    /// every test holds this lock for as long as it needs its own root.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Build a saves root containing one plain-directory entry with nested content.
    ///
    /// The returned guard must stay alive for the body of the test.
    fn fixture() -> (tempdir_shim::TempDir, std::sync::MutexGuard<'static, ()>) {
        // Recover rather than propagate if an earlier test panicked mid-lock.
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir_shim::TempDir::new();
        let game = tmp.path().join("Palworld_Server");
        std::fs::create_dir_all(game.join("Saved/nested")).unwrap();
        std::fs::write(game.join("Saved/level.sav"), b"level data").unwrap();
        std::fs::write(game.join("Saved/nested/deep.bin"), b"deep data").unwrap();
        std::env::set_var("GAME_SAVES_ROOT", tmp.path());
        (tmp, guard)
    }

    #[test]
    fn resolves_nested_path() {
        let (_tmp, _guard) = fixture();
        let resolved = resolve("Palworld_Server/Saved").unwrap();
        assert!(resolved.ends_with("Saved"));
        assert!(resolved.join("level.sav").exists());
    }

    #[test]
    fn rejects_parent_traversal() {
        let (_tmp, _guard) = fixture();
        let err = resolve("Palworld_Server/../../etc").unwrap_err();
        assert!(
            err.to_string().contains("not allowed"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn rejects_absolute_segment() {
        let (_tmp, _guard) = fixture();
        assert!(resolve("Palworld_Server//etc/passwd").is_err());
    }

    /// The whole point of the feature: an old backup replaces the live folder,
    /// stale files are gone, and the pre-restore zip can undo it.
    #[test]
    fn restore_sequence_replaces_and_is_undoable() {
        let (_tmp, _guard) = fixture();
        let dir = resolve("Palworld_Server/Saved").unwrap();

        // The archive `/backup` would have posted earlier.
        let old_backup = zip_dir(&dir).unwrap();

        // Time passes: a save is changed and a new file appears.
        std::fs::write(dir.join("level.sav"), b"NEW level data").unwrap();
        std::fs::write(dir.join("stale.tmp"), b"junk").unwrap();

        // Restore captures the current state first, then wipes and extracts.
        let pre_restore = zip_dir(&dir).unwrap();
        wipe_dir(&dir).unwrap();
        zip_extract::extract(std::io::Cursor::new(old_backup), &dir, false).unwrap();

        assert_eq!(std::fs::read(dir.join("level.sav")).unwrap(), b"level data");
        assert_eq!(
            std::fs::read(dir.join("nested/deep.bin")).unwrap(),
            b"deep data"
        );
        // A wipe-then-extract must not leave files the backup didn't contain.
        assert!(!dir.join("stale.tmp").exists());

        // Undo, using the safety backup exactly as restore would.
        wipe_dir(&dir).unwrap();
        zip_extract::extract(std::io::Cursor::new(pre_restore), &dir, false).unwrap();
        assert_eq!(
            std::fs::read(dir.join("level.sav")).unwrap(),
            b"NEW level data"
        );
        assert!(dir.join("stale.tmp").exists());
    }

    /// Every segment resolves the same way, so a link is followed at any depth —
    /// including one that leaves the entry, which the old containment check
    /// rejected. `.lnk` shortcuts take the same path but can only be verified on
    /// Windows, since directory shortcuts can't be written here.
    #[cfg(unix)]
    #[test]
    fn follows_a_link_partway_down_the_path() {
        let (tmp, _guard) = fixture();

        let outside = tmp.path().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("marker.txt"), b"found it").unwrap();

        let link = tmp.path().join("Palworld_Server/Linked");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let resolved = resolve("Palworld_Server/Linked").unwrap();
        assert_eq!(
            std::fs::read(resolved.join("marker.txt")).unwrap(),
            b"found it"
        );
    }

    /// An unreadable shortcut errors and names the file it choked on.
    #[test]
    fn bad_shortcut_reports_the_file() {
        let (tmp, _guard) = fixture();
        let bad = tmp.path().join("Broken.lnk");
        std::fs::write(&bad, b"not a shortcut").unwrap();

        let err = resolve_lnk(&bad).unwrap_err().to_string();
        assert!(err.contains("Broken.lnk"), "should name the file: {}", err);
    }

    #[test]
    fn filename_stamp_is_readable_and_sortable() {
        let stamp = filename_stamp();

        // YYYYMMDD-HHMMSS
        assert_eq!(stamp.len(), 15, "unexpected stamp: {}", stamp);
        assert_eq!(&stamp[8..9], "-", "unexpected stamp: {}", stamp);
        assert!(
            stamp
                .char_indices()
                .all(|(i, c)| i == 8 || c.is_ascii_digit()),
            "unexpected stamp: {}",
            stamp
        );

        // Lexical order must match chronological order, so backups sort by date.
        let year: i32 = stamp[..4].parse().unwrap();
        assert!((2026..2100).contains(&year), "unexpected year: {}", year);
    }

    #[test]
    fn sanitize_flattens_separators() {
        assert_eq!(sanitize("Palworld_Server/Saved"), "Palworld_Server_Saved");
        assert_eq!(sanitize("A\\B"), "A_B");
    }

    #[test]
    fn saves_root_errors_when_missing() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let missing = std::env::temp_dir().join("forest_bot_definitely_not_here");
        let _ = std::fs::remove_dir_all(&missing);
        std::env::set_var("GAME_SAVES_ROOT", &missing);

        let err = saves_root().unwrap_err().to_string();
        assert!(err.contains("does not exist"), "unexpected error: {}", err);
        // The override is the likely culprit, so the message should say so.
        assert!(err.contains("GAME_SAVES_ROOT"), "unexpected error: {}", err);
    }

    #[test]
    fn saves_root_errors_when_it_is_a_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempdir_shim::TempDir::new();
        let file = tmp.path().join("not_a_folder");
        std::fs::write(&file, b"x").unwrap();
        std::env::set_var("GAME_SAVES_ROOT", &file);

        let err = saves_root().unwrap_err().to_string();
        assert!(err.contains("not a folder"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_unknown_entry() {
        let (_tmp, _guard) = fixture();
        assert!(resolve("NotAGame/Saved").is_err());
    }

    #[test]
    fn zip_round_trips_through_extract() {
        let (tmp, _guard) = fixture();
        let src = resolve("Palworld_Server/Saved").unwrap();
        let bytes = zip_dir(&src).unwrap();

        let out = tmp.path().join("restored");
        std::fs::create_dir_all(&out).unwrap();
        zip_extract::extract(std::io::Cursor::new(bytes), &out, false).unwrap();

        assert_eq!(std::fs::read(out.join("level.sav")).unwrap(), b"level data");
        assert_eq!(
            std::fs::read(out.join("nested/deep.bin")).unwrap(),
            b"deep data"
        );
    }

    #[test]
    fn wipe_empties_without_removing_dir() {
        let (_tmp, _guard) = fixture();
        let dir = resolve("Palworld_Server/Saved").unwrap();
        wipe_dir(&dir).unwrap();
        assert!(dir.exists());
        assert!(is_empty(&dir).unwrap());
    }

    /// Minimal temp-dir helper so the crate doesn't need a dev-dependency.
    mod tempdir_shim {
        use std::path::{Path, PathBuf};

        pub struct TempDir(PathBuf);

        impl TempDir {
            pub fn new() -> Self {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let path = std::env::temp_dir().join(format!("forest_bot_test_{}", stamp));
                std::fs::create_dir_all(&path).unwrap();
                TempDir(path)
            }

            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
