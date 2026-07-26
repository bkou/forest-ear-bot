use std::path::Path;

use crate::util::saves;
use crate::{Context, Error};

/// Discord rejects messages over 2000 characters.
const MAX_MESSAGE: usize = 1900;

/// One line of a listing.
struct Row {
    /// Folders and shortcuts sort above plain files.
    enterable: bool,
    marker: &'static str,
    name: String,
    /// Trailing annotation: a size, or where a shortcut points.
    detail: String,
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    match bytes {
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.1} KB", b as f64 / KB as f64),
        b => format!("{} B", b),
    }
}

/// Describe everything in `dir`, resolving any shortcuts it contains.
///
/// Used for both the saves root and any folder below it — the root is only
/// special in that it's the one whose shortcuts `/backup` can be pointed at.
fn collect_rows(dir: &Path) -> Result<Vec<Row>, Error> {
    let mut rows = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        let row = if saves::is_lnk(&path) {
            // Show the target so a broken shortcut is obvious on sight.
            let detail = match saves::resolve_lnk(&path) {
                Ok(target) => format!(" -> {}", target.display()),
                Err(why) => format!(" -> unresolved ({})", why),
            };
            Row {
                enterable: true,
                marker: "[lnk]",
                name: name.strip_suffix(".lnk").unwrap_or(&name).to_string(),
                detail,
            }
        } else if entry.file_type()?.is_dir() {
            Row {
                enterable: true,
                marker: "[dir]",
                name: format!("{}/", name),
                detail: String::new(),
            }
        } else {
            Row {
                enterable: false,
                marker: "     ",
                name,
                detail: format!("  ({})", human_size(entry.metadata()?.len())),
            }
        };
        rows.push(row);
    }

    // Enterable things first, then alphabetical within each group.
    rows.sort_by(|a, b| b.enterable.cmp(&a.enterable).then(a.name.cmp(&b.name)));
    Ok(rows)
}

/// Format rows under `header`, stopping before Discord's message limit.
fn render_listing(header: &str, rows: &[Row]) -> String {
    let mut s = String::from(header);
    if rows.is_empty() {
        s.push_str("  (empty)\n");
    }
    for (i, row) in rows.iter().enumerate() {
        let line = format!("  {}  {}{}\n", row.marker, row.name, row.detail);
        if s.len() + line.len() > MAX_MESSAGE {
            s.push_str(&format!("  … and {} more\n", rows.len() - i));
            break;
        }
        s.push_str(&line);
    }
    s
}

/// List the available game folders, or the contents of one of them.
#[poise::command(prefix_command, slash_command)]
pub async fn list_saves(
    ctx: Context<'_>,
    #[description = "Folder to list, e.g. Palworld_Server/Saved — omit to list the root"]
    path: Option<String>,
) -> Result<(), Error> {
    let path = path.as_deref().map(str::trim).filter(|p| !p.is_empty());

    // The only difference between the two modes is where we start.
    let dir = match path {
        Some(path) => saves::resolve(path)?,
        None => saves::saves_root()?,
    };

    let header = match path {
        Some(path) => format!("`{}` ({}):\n", path, dir.display()),
        None => format!("Save entries in {}:\n", dir.display()),
    };

    let mut body = render_listing(&header, &collect_rows(&dir)?);
    body.push_str(&match path {
        Some(path) => format!(
            "\nBack this up with `/backup {}`, or a single file with `/backup {}/<file>`.",
            path, path
        ),
        None => "\nList a folder with `/list_saves <entry>/<subfolder>`.".to_string(),
    });

    ctx.say(body).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_row(name: &str, size: u64) -> Row {
        Row {
            enterable: false,
            marker: "     ",
            name: name.to_string(),
            detail: format!("  ({})", human_size(size)),
        }
    }

    fn dir_row(name: &str) -> Row {
        Row {
            enterable: true,
            marker: "[dir]",
            name: format!("{}/", name),
            detail: String::new(),
        }
    }

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn long_listing_stays_under_the_limit() {
        let rows: Vec<Row> = (0..500)
            .map(|i| file_row(&format!("save_file_number_{:04}.sav", i), 1024))
            .collect();

        let out = render_listing("header\n", &rows);

        assert!(out.len() <= MAX_MESSAGE + 32, "too long: {}", out.len());
        assert!(out.contains("more"), "expected a truncation note");
        assert!(out.contains("save_file_number_0000.sav"));
        assert!(!out.contains("save_file_number_0499.sav"));
    }

    #[test]
    fn short_listing_is_not_truncated() {
        let rows = vec![dir_row("nested"), file_row("level.sav", 2048)];
        let out = render_listing("header\n", &rows);
        assert!(out.contains("[dir]  nested/"));
        assert!(out.contains("level.sav  (2.0 KB)"));
        assert!(!out.contains("more"));
    }

    #[test]
    fn empty_listing_says_so() {
        assert!(render_listing("header\n", &[]).contains("(empty)"));
    }

    #[test]
    fn shortcut_row_shows_its_target() {
        let rows = vec![Row {
            enterable: true,
            marker: "[lnk]",
            name: String::from("Palworld_Server"),
            detail: String::from(" -> D:\\Games\\Palworld"),
        }];
        let out = render_listing("header\n", &rows);
        assert!(out.contains("[lnk]  Palworld_Server -> D:\\Games\\Palworld"));
    }

    #[test]
    fn enterable_entries_sort_before_files() {
        let mut rows = [
            file_row("a_file", 1),
            dir_row("z_dir"),
            file_row("b_file", 1),
            dir_row("a_dir"),
        ];
        rows.sort_by(|a, b| b.enterable.cmp(&a.enterable).then(a.name.cmp(&b.name)));
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a_dir/", "z_dir/", "a_file", "b_file"]);
    }
}
