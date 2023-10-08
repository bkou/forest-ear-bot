use screenshots::Screen;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::env;

#[command]
pub async fn screenshot(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let screens = Screen::all().unwrap();
    let image = screens[0].capture().unwrap();
    let temp_file = env::temp_dir().join("forest_bot_screenshot.png");
    image.save(&temp_file).unwrap();

    let msg = msg
        .channel_id
        .send_message(&ctx.http, |m| {
            m.embed(|e| {
                e.image("attachment://forest_bot_screnshot.png")
                    .footer(|f| f.text(format!("{}", temp_file.display())))
                    // Add a timestamp for the current time
                    // This also accepts a r/fc3339 Timestamp
                    .timestamp(Timestamp::now())
            })
            .add_file(&temp_file)
        })
        .await;

    if let Err(why) = msg {
        println!("Error sending message: {:?}", why);
    }

    Ok(())
}
