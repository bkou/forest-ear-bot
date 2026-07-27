use directories::UserDirs;
use std::process::Command;

use crate::util::saves;
use crate::{Context, Error};

/// Run a desktop shortcut (.lnk) by filename.
#[poise::command(prefix_command, slash_command)]
pub async fn run_desktop_shortcut(
    ctx: Context<'_>,
    #[description = "Shortcut filename on the desktop (no directory)"] filename: String,
) -> Result<(), Error> {
    if let Some(user_dirs) = UserDirs::new() {
        let path = user_dirs.desktop_dir().unwrap().join(&filename);

        let target = saves::resolve_lnk(&path)?;

        // Detaching is the point — these are games and servers meant to outlive
        // the command. Waiting would block until the user quits the program.
        // Clippy warns because an unreaped child becomes a zombie on Unix; on
        // Windows, the only platform where `.lnk` resolution works, dropping the
        // handle is a complete cleanup.
        #[allow(clippy::zombie_processes)]
        let _child = Command::new(&target)
            .spawn()
            .map_err(|e| format!("Could not start {}: {}", target.display(), e))?;

        ctx.say(format!("Started {}", filename)).await?;
    }

    Ok(())
}
