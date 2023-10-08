use serenity::framework::standard::macros::command;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use sysinfo::{ProcessExt, System, SystemExt};

#[command]
pub async fn kill_process(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let process_name = args.single::<String>()?;

    let sys = System::new_all();

    let mut s = String::new();
    s.push_str("Found:\n");
    for process in sys.processes_by_name(process_name.as_str()) {
        println!(
            "killing [{}] {} {:?}",
            process.pid(),
            process.name(),
            process.disk_usage()
        );
        s.push_str(&format!(
            "killing [{}] {} {:?}",
            process.pid(),
            process.name(),
            process.disk_usage()
        ));
        s.push_str("\n");
        process.kill();
    }
    if let Err(why) = msg.channel_id.say(&ctx.http, &s).await {
        println!("Error sending message: {:?}", why);
    }
    println!("{}", s);

    Ok(())
}
