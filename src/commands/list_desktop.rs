use directories::UserDirs;
use std::fs;

use crate::{Context, Error};

/// List the files on the desktop.
#[poise::command(prefix_command, slash_command)]
pub async fn list_desktop(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(user_dirs) = UserDirs::new() {
        let paths = fs::read_dir(user_dirs.desktop_dir().unwrap().as_os_str()).unwrap();

        let mut s = String::new();
        for path in paths {
            s.push_str(format!("{}", path.unwrap().path().display()).as_str());
            s.push_str("\n");
        }
        s.push_str("\n");
        s.push_str("Run the run_desktop_shortcut with just the filename (no directory) as arg.");
        ctx.say(&s).await?;
    }

    Ok(())
}
