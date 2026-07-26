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
        Command::new(&target)
            .spawn()
            .expect("command failed to start");

        ctx.say(format!("Started {}", filename)).await?;
    }

    Ok(())
}
