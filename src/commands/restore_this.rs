use directories::UserDirs;
use lnk::ShellLink;
use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// Download the single zip attached to `source` and extract it into the saves folder.
///
/// Shared by both entry points: the reply-based prefix command and the message
/// context menu action.
async fn restore_from_message(ctx: Context<'_>, source: &serenity::Message) -> Result<(), Error> {
    let attachments = &source.attachments;
    if attachments.len() != 1 {
        ctx.say("Expecting exactly one attachment").await?;
        return Ok(());
    }

    let attachment = &attachments[0];

    let content = match attachment.download().await {
        Ok(content) => content,
        Err(_) => {
            ctx.say("Error downloading attachment").await?;
            return Ok(());
        }
    };

    // The filename is prefixed with the server number (e.g. "1_save.zip"); the
    // original intent was to extract into a per-server subfolder, never finished.
    ctx.say("downloaded zip content to var").await?;

    let user_dirs = UserDirs::new().unwrap();
    let saves_dir_link_path = user_dirs.desktop_dir().unwrap().join("forest_saves.lnk");
    let saves_dir_link = ShellLink::open(saves_dir_link_path).expect("couldn't open shell link");
    let saves_dir_string = saves_dir_link.relative_path().as_ref().unwrap().clone();
    let saves_dir = std::path::Path::new(&saves_dir_string);

    // Delete and restore new save.
    ctx.say("starting extract").await?;
    let _ = zip_extract::extract(std::io::Cursor::new(content), saves_dir, false);
    ctx.say("done with extract, save should be done").await?;

    Ok(())
}

/// Restore the save zip attached to the message you replied to.
#[poise::command(prefix_command)]
pub async fn restore_this(ctx: Context<'_>) -> Result<(), Error> {
    // Replying is a prefix-command concept, so reach for the underlying message.
    let msg = match ctx {
        poise::Context::Prefix(prefix) => prefix.msg,
        _ => {
            ctx.say("This command must be used as a reply.").await?;
            return Ok(());
        }
    };

    let Some(reference) = &msg.message_reference else {
        ctx.say("Not a reply?").await?;
        return Ok(());
    };

    let orig_message = msg
        .channel_id
        .message(ctx.http(), reference.message_id.unwrap())
        .await?;

    ctx.say("starting").await?;
    restore_from_message(ctx, &orig_message).await
}

/// Restore the save zip attached to the right-clicked message.
#[poise::command(context_menu_command = "Restore this save")]
pub async fn restore_this_menu(ctx: Context<'_>, message: serenity::Message) -> Result<(), Error> {
    ctx.say("starting").await?;
    restore_from_message(ctx, &message).await
}
