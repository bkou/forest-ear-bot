use sysinfo::{ProcessExt, System, SystemExt};

use crate::{Context, Error};

/// Kill running processes whose name matches the argument.
#[poise::command(prefix_command, slash_command)]
pub async fn kill_process(
    ctx: Context<'_>,
    #[description = "Process name to kill"] process_name: String,
) -> Result<(), Error> {
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
        s.push('\n');
        process.kill();
    }
    ctx.say(&s).await?;
    println!("{}", s);

    Ok(())
}
