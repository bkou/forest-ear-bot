use directories::UserDirs;
use lnk::ShellLink;

use crate::{Context, Error};

/// Restore the save zip attached to the replied-to message (prefix reply only).
#[poise::command(prefix_command)]
pub async fn restore_this(ctx: Context<'_>) -> Result<(), Error> {
    // This command only makes sense as a reply to a message, which is a
    // prefix-command concept, so grab the underlying serenity message.
    let msg = match ctx {
        poise::Context::Prefix(prefix) => prefix.msg,
        _ => {
            ctx.say("This command must be used as a reply.").await?;
            return Ok(());
        }
    };

    if let Some(reference) = &msg.message_reference {
        let orig_message = msg
            .channel_id
            .message(ctx.http(), reference.message_id.unwrap())
            .await
            .unwrap();
        let _text = orig_message.content;
        ctx.say("starting").await?;

        let attachments = orig_message.attachments;
        if attachments.len() != 1 {
            ctx.say("Expecting exactly one attachment").await?;
            return Ok(());
        }

        ctx.say("got attachment").await?;

        let attachment = &attachments[0];

        let content = match attachment.download().await {
            Ok(content) => content,
            Err(_) => {
                ctx.say("Error downloading attachment").await?;
                return Ok(());
            }
        };

        let _server_num = attachment.filename.split("_").collect::<Vec<_>>()[0];

        ctx.say("downloaded zip content to var").await?;

        let user_dirs = UserDirs::new().unwrap();
        let saves_dir_link_path = user_dirs.desktop_dir().unwrap().join("forest_saves.lnk");
        let saves_dir_link =
            ShellLink::open(saves_dir_link_path).expect("couldn't open shell link");
        let saves_dir_string = saves_dir_link.relative_path().as_ref().unwrap().clone();
        let saves_dir = std::path::Path::new(&saves_dir_string);

        // Delete and restore new save.
        ctx.say("starting extract").await?;
        let _ = zip_extract::extract(std::io::Cursor::new(content), saves_dir, false);
        ctx.say("done with extract, save should be done").await?;

        Ok(())
    } else {
        ctx.say("Not a reply?").await?;
        Ok(())
    }
}
