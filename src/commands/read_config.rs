use directories::UserDirs;
use std::fs;

use crate::util::saves;
use crate::{Context, Error};

/// Read and post the dedicated server config file.
#[poise::command(prefix_command, slash_command)]
pub async fn read_config(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(user_dirs) = UserDirs::new() {
        let path = user_dirs
            .desktop_dir()
            .unwrap()
            .join("dedicatedserver.cfg.lnk");

        let target = match saves::resolve_lnk(&path) {
            Err(why) => {
                ctx.say(format!("File not found: {}", why)).await?;
                return Err(Error::from("File not found."));
            }
            Ok(target) => target,
        };
        let contents = fs::read_to_string(&target)?;

        ctx.say("File contents: ").await?;
        ctx.say(contents).await?;
    }
    Ok(())
}
