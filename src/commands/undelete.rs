use poise::serenity_prelude as serenity;

use crate::util::saves;
use crate::{Context, Error};

/// Put back the folder or file from the message you replied to.
#[poise::command(prefix_command)]
pub async fn undelete(ctx: Context<'_>) -> Result<(), Error> {
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

    undelete_from_message(ctx, &orig_message).await
}

/// Put back the folder or file from the right-clicked message.
#[poise::command(context_menu_command = "Undelete this")]
pub async fn undelete_menu(ctx: Context<'_>, message: serenity::Message) -> Result<(), Error> {
    undelete_from_message(ctx, &message).await
}

/// Recreate whatever a backup message holds, at the path it records.
///
/// Unlike restore, this refuses when something is already there. Undelete is for
/// putting back something that's gone; overwriting live data is restore's job,
/// and that path takes a safety copy first. Doing it here would destroy data
/// with no undo.
async fn undelete_from_message(ctx: Context<'_>, source: &serenity::Message) -> Result<(), Error> {
    ctx.defer().await?;

    let Some(path) = saves::parse_backup_path(source) else {
        ctx.say("That isn't a backup message from this bot — undelete only works on messages this bot posted.")
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

    // Resolves the parent only, since the leaf is expected to be missing.
    let target = saves::resolve_new(&path)?;

    if target.exists() {
        ctx.say(format!(
            "`{}` already exists — refusing to overwrite it. Use the Restore action instead; \
             it takes a backup of the current contents first.",
            path
        ))
        .await?;
        return Ok(());
    }

    let content = attachment
        .download()
        .await
        .map_err(|e| format!("Error downloading attachment: {}", e))?;

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        match kind {
            // No need to create the folder first — extract() makes the target
            // directory, and resolve_new guarantees its parent exists.
            saves::Kind::Folder => {
                zip_extract::extract(std::io::Cursor::new(content), &target, false)
                    .map_err(|e| format!("{}", e))?;
            }
            saves::Kind::File => {
                std::fs::write(&target, content).map_err(|e| format!("{}", e))?;
            }
        }
        Ok(())
    })
    .await??;

    ctx.say(format!("Undeleted {} `{}`.", kind.as_str(), path))
        .await?;
    Ok(())
}
