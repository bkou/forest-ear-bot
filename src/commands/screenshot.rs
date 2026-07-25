use poise::serenity_prelude as serenity;
use screenshots::Screen;
use std::env;

use crate::{Context, Error};

/// Capture the primary screen and post it as an image.
#[poise::command(prefix_command, slash_command)]
pub async fn screenshot(ctx: Context<'_>) -> Result<(), Error> {
    let screens = Screen::all().unwrap();
    let image = screens[0].capture().unwrap();
    let temp_file = env::temp_dir().join("forest_bot_screenshot.png");
    image.save(&temp_file).unwrap();

    let attachment = serenity::CreateAttachment::path(&temp_file).await?;
    let embed = serenity::CreateEmbed::new()
        .image("attachment://forest_bot_screenshot.png")
        .footer(serenity::CreateEmbedFooter::new(format!(
            "{}",
            temp_file.display()
        )))
        // Add a timestamp for the current time.
        .timestamp(serenity::Timestamp::now());

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .attachment(attachment),
    )
    .await?;

    Ok(())
}
