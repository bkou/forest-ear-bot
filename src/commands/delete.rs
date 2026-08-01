use crate::util::{locks, saves};
use crate::{Context, Error};

/// Post a folder or file, then delete it.
#[poise::command(prefix_command, slash_command)]
pub async fn delete(
    ctx: Context<'_>,
    #[description = "Folder or file to delete, e.g. Palworld_Server/Saved"] path: String,
    #[description = "Optional note about why it was deleted"]
    #[rest]
    description: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    // Deleting a shortcut entry would wipe the game folder it points at, not the
    // shortcut — far more than anyone typing this would expect.
    if saves::leaf_is_shortcut(&path)? {
        ctx.say(format!(
            "`{}` is a shortcut. Deleting it would remove everything it points at, \
             not the shortcut itself — refusing. Delete something inside it instead, \
             like `{}/<folder>`.",
            path, path
        ))
        .await?;
        return Ok(());
    }

    let target = saves::resolve(&path)?;
    let kind = saves::Kind::of(&target);
    let game_dir = saves::game_dir(&path).ok();

    // A save the game server still has open can be read but not deleted, so
    // without this the command uploads a whole backup and only then discovers it
    // cannot finish. Check while refusing is still free.
    let probe_target = target.clone();
    if let Some(locked) =
        tokio::task::spawn_blocking(move || locks::first_locked(&probe_target)).await?
    {
        let holder = game_dir
            .as_deref()
            .and_then(locks::process_under)
            .map(|process| format!(" `{}` is the likely holder.", process))
            .unwrap_or_default();
        ctx.say(format!(
            "`{}` can't be deleted right now: `{}` is open in another process, so Windows \
             will refuse to remove it.{} Stop the game server and try again — nothing has \
             been backed up or deleted.",
            path,
            locked.display(),
            holder
        ))
        .await?;
        return Ok(());
    }

    // Upload first. If this fails the data is still on disk, so nothing is lost
    // — the same ordering that makes restore safe.
    saves::post_backup(
        ctx,
        &path,
        &target,
        "Deleted",
        description.as_deref().or(Some(
            "Deleted from disk. Use the Undelete action on this message to put it back.",
        )),
    )
    .await?;

    tokio::task::spawn_blocking(move || {
        saves::delete_path(&target, game_dir.as_deref()).map_err(|e| format!("{}", e))
    })
    .await??;

    ctx.say(format!("Deleted {} `{}`.", kind.as_str(), path))
        .await?;
    Ok(())
}
