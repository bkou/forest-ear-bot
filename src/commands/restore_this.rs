use directories::UserDirs;
use lnk::ShellLink;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use sysinfo::{ProcessExt, System, SystemExt};
use std::env;
use tokio::io::AsyncWriteExt;


#[command]
pub async fn restore_this(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {


    if let Some(reference) = &msg.message_reference {
        let orig_message = reference.channel_id.message(&ctx.http, reference.message_id.unwrap()).await.unwrap();
        let text = orig_message.content;
        let _ = &msg.channel_id.say(&ctx.http, "starting").await;

        let attachments = orig_message.attachments;
        if attachments.len() != 1 {
            let _ = &msg.channel_id.say(&ctx.http, "Expecting exactly one attachment").await;
            return Ok(());
        }

        let _ = &msg.channel_id.say(&ctx.http, "got attachment").await;

        let attachment = &attachments[0];

        let content = match attachment.download().await {
            Ok(content) => content,
            Err(_) => {
                let _ =
                   msg.channel_id.say(&ctx, "Error downloading attachment").await;

                return Ok(());
            },
        };

        let server_num = attachment.filename.split("_").collect::<Vec<_>>()[0];

        let _ = &msg.channel_id.say(&ctx.http, "downloaded zip content to var").await;


        let user_dirs = UserDirs::new().unwrap();
        let saves_dir_link = ShellLink::open(saves_dir_link).expect("couldn't open shell link");
        let saves_dir_string = saves_dir_link.relative_path().as_ref().unwrap();
        let saves_dir = std::path::Path::new(&saves_dir_string);
        //let save_dir_path = &saves_dir.join(server_num);

        // Delete and restore new save.
        let _ = &msg.channel_id.say(&ctx.http, "starting extract").await;
        let _ = zip_extract::extract(std::io::Cursor::new(content), &saves_dir, false);
        let _ = &msg.channel_id.say(&ctx.http, "done with extract, save should be done").await;

        return Ok(());
    } else {
        if let Err(why) = msg.channel_id.say(&ctx.http, "Not a reply?").await {
            println!("Error sending message: {:?}", why);
        }
        return Ok(());
    }
}
