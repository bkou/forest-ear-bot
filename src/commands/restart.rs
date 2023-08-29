use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use system_shutdown::shutdown;

#[command]
pub async fn restart(_ctx: &Context, _msg: &Message, _args: Args) -> CommandResult {
    match shutdown() {
        Ok(_) => println!("Shutting down, bye!"),
        Err(error) => eprintln!("Failed to shut down: {}", error),
    }

    Ok(())
}
