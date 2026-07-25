use system_shutdown::reboot;

use crate::{Context, Error};

/// Reboot the machine the bot runs on.
#[poise::command(prefix_command, slash_command)]
pub async fn restart_server(_ctx: Context<'_>) -> Result<(), Error> {
    match reboot() {
        Ok(_) => println!("Shutting down, bye!"),
        Err(error) => eprintln!("Failed to shut down: {}", error),
    }

    Ok(())
}
