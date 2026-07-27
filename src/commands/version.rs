use crate::{Context, Error};

/// Report the version and git revision this bot was built from.
#[poise::command(prefix_command, slash_command)]
pub async fn version(ctx: Context<'_>) -> Result<(), Error> {
    // Baked in at compile time by build.rs.
    ctx.say(format!(
        "{} v{} — built from `{}`",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
    ))
    .await?;
    Ok(())
}
