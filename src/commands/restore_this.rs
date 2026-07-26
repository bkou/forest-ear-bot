use poise::serenity_prelude as serenity;

use crate::util::saves;
use crate::{Context, Error};

/// Replace a save folder or file from the attachment on a backup message.
///
/// The destination and kind come from the message itself rather than arguments,
/// which is what lets this work as a context menu action (those take no
/// parameters).
///
/// The current contents are posted *before* anything is overwritten, so a
/// failure at any point leaves the filesystem untouched and every restore can be
/// undone from the message it posts.
async fn restore_from_message(ctx: Context<'_>, source: &serenity::Message) -> Result<(), Error> {
    ctx.defer().await?;

    let Some(path) = saves::parse_backup_path(source) else {
        ctx.say("That isn't a backup message from this bot — restore only works on messages `/backup` posted.")
            .await?;
        return Ok(());
    };
    let kind = saves::parse_backup_kind(source);

    let attachments = &source.attachments;
    if attachments.len() != 1 {
        ctx.say("Expecting exactly one attachment").await?;
        return Ok(());
    }
    let attachment = &attachments[0];

    let target = saves::resolve(&path)?;

    // The backup and the destination must agree, or we'd unzip over a file or
    // write an archive where a folder belongs.
    if saves::Kind::of(&target) != kind {
        ctx.say(format!(
            "`{}` is now a {}, but that backup holds a {}. Refusing to restore.",
            path,
            saves::Kind::of(&target).as_str(),
            kind.as_str()
        ))
        .await?;
        return Ok(());
    }

    let content = attachment
        .download()
        .await
        .map_err(|e| format!("Error downloading attachment: {}", e))?;

    // Safety copy first — if this upload fails, we abort having destroyed nothing.
    if kind == saves::Kind::Folder && saves::is_empty(&target)? {
        ctx.say(format!("`{}` is empty, nothing to save first.", path))
            .await?;
    } else {
        saves::post_backup(
            ctx,
            &path,
            &target,
            "Pre-restore backup",
            Some("Automatic snapshot taken just before a restore. Restore this message to undo."),
        )
        .await?;
    }

    let dest = target.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        match kind {
            // Wipe first so files the backup doesn't contain don't survive.
            saves::Kind::Folder => {
                saves::wipe_dir(&dest).map_err(|e| format!("{}", e))?;
                zip_extract::extract(std::io::Cursor::new(content), &dest, false)
                    .map_err(|e| format!("{}", e))?;
            }
            saves::Kind::File => {
                std::fs::write(&dest, content).map_err(|e| format!("{}", e))?;
            }
        }
        Ok(())
    })
    .await??;

    ctx.say(format!("Restored `{}`.", path)).await?;
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

    restore_from_message(ctx, &orig_message).await
}

/// Restore the save zip attached to the right-clicked message.
#[poise::command(context_menu_command = "Restore this save")]
pub async fn restore_this_menu(ctx: Context<'_>, message: serenity::Message) -> Result<(), Error> {
    restore_from_message(ctx, &message).await
}
