use directories::UserDirs;
use lnk::ShellLink;
use std::fs;

use crate::{Context, Error};

/// Read and post the dedicated server config file.
#[poise::command(prefix_command, slash_command)]
pub async fn read_config(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(user_dirs) = UserDirs::new() {
        let path = user_dirs
            .desktop_dir()
            .unwrap()
            .join("dedicatedserver.cfg.lnk");

        let deref = match ShellLink::open(path) {
            Err(_) => {
                ctx.say("File not found.").await?;
                return Err(Error::from("File not found."));
            }
            Ok(f) => f,
        };
        let contents = fs::read_to_string(deref.relative_path().as_ref().unwrap())?;

        ctx.say("File contents: ").await?;
        ctx.say(contents).await?;
    }
    Ok(())
}
