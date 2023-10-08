use directories::UserDirs;
use lnk::ShellLink;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::error::Error;
use std::fs;

#[command]
pub async fn backup(ctx: &Context, msg: &Message) -> CommandResult {
    if let Some(user_dirs) = UserDirs::new() {
        let save_dir = user_dirs.desktop_dir().unwrap().join("forest_saves.lnk");

        let deref = match ShellLink::open(save_dir) {
            Err(_) => {
                if let Err(why) = msg
                    .channel_id
                    .say(&ctx.http, "Save folder not found.")
                    .await
                {
                    println!("Error sending message: {:?}", why);
                }
                // Seriously I have no idea how to return the error.
                return Err(Box::<dyn Error + std::marker::Send + Sync>::from(
                    "File not found.",
                ));
            }
            Ok(f) => f,
        };

        let mut paths = fs::read_dir(deref.relative_path().as_ref().unwrap())?
            .filter(|p| {
                p.as_ref()
                    .unwrap()
                    .path()
                    .to_str()
                    .unwrap()
                    .contains(".zip")
            })
            .collect::<Vec<_>>();
        println!("{:?}", paths);
        println!(
            "{:?}",
            paths.sort_by(|a, b| a
                .as_ref()
                .unwrap()
                .metadata()
                .unwrap()
                .created()
                .unwrap()
                .cmp(&b.as_ref().unwrap().metadata().unwrap().created().unwrap()))
        );
        let last_backup = &paths.last().unwrap();
        // No fucking clue why this can't go in the block, has to be defined or "does not live long
        // enough".
        let path = last_backup.as_ref().clone().unwrap().path();
        let path_str = String::from(path.clone().to_str().unwrap());
        if let Err(why) = msg
            .channel_id
            .send_message(
                &ctx.http, |m| {
                    m.content(format!("{:?}", path_str))
                        .add_file(path_str.as_str())
            })
            .await
        {
            println!("Error sending message: {:?}", why);
        }
    }
    Ok(())
}
