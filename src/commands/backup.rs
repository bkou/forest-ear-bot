use crate::util::saves;
use crate::{Context, Error};

/// Zip a save folder and post it, e.g. `Palworld_Server/Saved`.
#[poise::command(prefix_command, slash_command)]
pub async fn backup(
    ctx: Context<'_>,
    #[description = "Save folder to back up, e.g. Palworld_Server/Saved"] path: String,
    // `rest` so a prefix invocation takes the whole remaining message instead of
    // stopping at the first space.
    #[description = "Optional note to tag this backup with"]
    #[rest]
    description: Option<String>,
) -> Result<(), Error> {
    // Zipping easily outruns Discord's 3s interaction deadline.
    ctx.defer().await?;

    let resolved_path = saves::resolve(&path)?;
    saves::post_backup(ctx, &path, &resolved_path, "Backup", description.as_deref()).await
}
