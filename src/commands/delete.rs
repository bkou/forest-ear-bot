use crate::util::saves;
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

    tokio::task::spawn_blocking(move || saves::delete_path(&target).map_err(|e| format!("{}", e)))
        .await??;

    ctx.say(format!("Deleted {} `{}`.", kind.as_str(), path))
        .await?;
    Ok(())
}
