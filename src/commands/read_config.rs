use directories::UserDirs;
use lnk::ShellLink;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::error::Error;
use std::fs;

#[command]
pub async fn read_config(ctx: &Context, msg: &Message) -> CommandResult {
    if let Some(user_dirs) = UserDirs::new() {
        let path = user_dirs
            .desktop_dir()
            .unwrap()
            .join("dedicatedserver.cfg.lnk");

        let deref = match ShellLink::open(path) {
            Err(_) => {
                if let Err(why) = msg.channel_id.say(&ctx.http, "File not found.").await {
                    println!("Error sending message: {:?}", why);
                }
                // Seriously I have no idea how to return the error.
                return Err(Box::<dyn Error + std::marker::Send + Sync>::from(
                    "File not found.",
                ));
            }
            Ok(f) => f,
        };
        let contents = fs::read_to_string(&deref.relative_path().as_ref().unwrap())?;

        if let Err(why) = msg.channel_id.say(&ctx.http, "File contents: ").await {
            println!("Error sending message: {:?}", why);
        }
        if let Err(why) = msg.channel_id.say(&ctx.http, contents).await {
            println!("Error sending message: {:?}", why);
        }
    }
    Ok(())
}
