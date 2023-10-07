use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use directories::UserDirs;
use std::process::Command;
use lnk::ShellLink;

#[command]
pub async fn run_desktop_shortcut(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {

    if let Some(user_dirs) = UserDirs::new() {
        let filename = args.single::<String>()?;
        let path = user_dirs.desktop_dir().unwrap().join(&filename);

        let deref = ShellLink::open(path).unwrap();
        Command::new(deref.relative_path().clone().unwrap())
            .spawn()
            .expect("command failed to start");

        if let Err(why) = msg.channel_id.say(&ctx.http, format!("Started {}", filename)).await {
            println!("Error sending message: {:?}", why);
        }
    }


    Ok(())
}
