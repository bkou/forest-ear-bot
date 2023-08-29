use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::fs;
use directories::UserDirs;

#[command]
pub async fn list_desktop(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {

    if let Some(user_dirs) = UserDirs::new() {
        let paths = fs::read_dir(user_dirs.desktop_dir().unwrap().as_os_str()).unwrap();

        let mut s = String::new();
        for path in paths {
            s.push_str(format!("{}", path.unwrap().path().display()).as_str());
            s.push_str("\n");
        }
        s.push_str("\n");
        s.push_str("Run the run_desktop_shortcut with just the filename (no directory) as arg.");
        if let Err(why) = msg.channel_id.say(&ctx.http, &s).await {
            println!("Error sending message: {:?}", why);
        }
    }


    Ok(())
}
