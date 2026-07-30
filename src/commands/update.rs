use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use directories::UserDirs;
use poise::serenity_prelude as serenity;
use tokio::process::Command;

use crate::{Context, Error};

/// How long SteamCMD may run before we give up and kill it. Overridable with
/// `STEAMCMD_TIMEOUT_SECS`, since a big update on a slow connection can
/// legitimately take a long time.
const DEFAULT_TIMEOUT_SECS: u64 = 30 * 60;

/// Run SteamCMD with the given arguments and report the result.
///
/// e.g. `/update +login anonymous +app_update 2394010 validate +quit`
#[poise::command(prefix_command, slash_command)]
pub async fn update(
    ctx: Context<'_>,
    #[description = "SteamCMD arguments, e.g. +login anonymous +app_update 2394010 validate +quit"]
    #[rest]
    arguments: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let args: Vec<&str> = arguments.split_whitespace().collect();
    if let Some(reason) = invalid_reason(&args) {
        ctx.say(reason).await?;
        return Ok(());
    }

    let exe = steamcmd_executable()?;
    let timeout_secs = std::env::var("STEAMCMD_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    ctx.say(format!(
        "Running SteamCMD (killed if it runs past {} minutes)…",
        timeout_secs / 60
    ))
    .await?;

    // SteamCMD's own log file is more trustworthy than its piped stdout/stderr
    // — see console_log_path's doc comment — so note how much of it already
    // exists before running, and read only what gets appended during this run.
    let console_log = console_log_path(&exe);
    let console_log_start = file_len(&console_log);

    // The final report goes out as a plain channel message rather than a
    // reply to the interaction. SteamCMD can run well past Discord's 15-minute
    // interaction token lifetime, after which an interaction followup would
    // simply fail to send — a normal message using the bot's own token has no
    // such limit.
    let outcome = run_steamcmd(&exe, &args, timeout_secs).await;
    let console_output = read_appended(&console_log, console_log_start);
    ctx.channel_id()
        .send_message(ctx.http(), render_result(&args, outcome, console_output))
        .await?;

    Ok(())
}

/// Rejects the request before touching the filesystem or spawning anything.
/// `+quit` is required — not just recommended — because without it SteamCMD
/// drops into an interactive prompt and never exits on its own. The timeout
/// would eventually kill it regardless, but there's no reason to make the
/// user wait 30 minutes to find out they left off one token.
fn invalid_reason(args: &[&str]) -> Option<&'static str> {
    if args.is_empty() {
        return Some(
            "No arguments given. SteamCMD needs at least `+quit`, or it will \
             sit waiting for interactive input forever.",
        );
    }
    if !args.contains(&"+quit") {
        return Some(
            "Missing `+quit`. Without it SteamCMD drops into an interactive \
             prompt and never exits on its own — add `+quit` at the end.",
        );
    }
    None
}

/// Locate the SteamCMD executable: `STEAMCMD_PATH` if set, else
/// `Desktop/steamcmd/steamcmd.exe`.
fn steamcmd_executable() -> Result<PathBuf, Error> {
    let path = match std::env::var("STEAMCMD_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let user_dirs = UserDirs::new().ok_or("Could not locate the user directories")?;
            let desktop = user_dirs
                .desktop_dir()
                .ok_or("Could not locate the desktop directory")?;
            desktop.join("steamcmd").join("steamcmd.exe")
        }
    };
    validate_executable(path)
}

fn validate_executable(path: PathBuf) -> Result<PathBuf, Error> {
    if !path.exists() {
        return Err(format!(
            "SteamCMD not found at {}. Set STEAMCMD_PATH, or place it at \
             Desktop/steamcmd/steamcmd.exe.",
            path.display()
        )
        .into());
    }
    if !path.is_file() {
        return Err(format!("{} is not a file.", path.display()).into());
    }
    Ok(path)
}

/// What came of trying to run SteamCMD.
enum Outcome {
    Ran(std::process::Output),
    TimedOut,
    FailedToStart(std::io::Error),
}

/// Run SteamCMD, bounded by `timeout_secs`.
///
/// `kill_on_drop` is what makes the timeout actually terminate a hung process:
/// when the timeout elapses, the output future is dropped, and with
/// `kill_on_drop` set that drop kills the child instead of leaving it running
/// unattended in the background.
async fn run_steamcmd(exe: &Path, args: &[&str], timeout_secs: u64) -> Outcome {
    let mut command = Command::new(exe);
    command
        .args(args)
        .current_dir(exe.parent().unwrap_or_else(|| Path::new(".")))
        // No stdin: without a `+quit`, SteamCMD would otherwise sit waiting on
        // interactive input that's never coming.
        .stdin(Stdio::null())
        .kill_on_drop(true);

    match tokio::time::timeout(Duration::from_secs(timeout_secs), command.output()).await {
        Err(_) => Outcome::TimedOut,
        Ok(Err(e)) => Outcome::FailedToStart(e),
        Ok(Ok(output)) => Outcome::Ran(output),
    }
}

/// Where SteamCMD writes its own persistent console log, as it reports itself:
/// `Logging directory: '<steamcmd dir>/logs'`.
///
/// SteamCMD's piped stdout/stderr — what `run_steamcmd` captures directly —
/// isn't reliable for the tail end of a run: like many console apps, it
/// buffers differently once its output is redirected to a pipe rather than a
/// real terminal, and can exit without flushing the last lines through. This
/// file is SteamCMD's own record of the same session, written through a
/// separate path that doesn't have that problem, so it's the more trustworthy
/// source for what actually happened — particularly the final status line.
fn console_log_path(exe: &Path) -> PathBuf {
    exe.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("logs")
        .join("console_log.txt")
}

/// The file's current length, or 0 if it doesn't exist yet.
fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The bytes written to `path` after `offset`.
///
/// `console_log.txt` is appended to forever across every run, so reading the
/// whole file every time would grow without bound; this returns just what one
/// run added. Any failure to read — the file doesn't exist, a race on an
/// unusual SteamCMD version — degrades to an empty result rather than an
/// error, since this is a supplementary log, not one the command depends on.
fn read_appended(path: &Path, offset: u64) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};

    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    let _ = file.read_to_end(&mut buf);
    buf
}

/// Discord messages are capped at 2000 characters; keep the echoed invocation
/// well under that so the status line itself can never fail to send.
const MAX_INVOCATION_DISPLAY: usize = 1500;

fn truncate_invocation(invocation: &str) -> String {
    if invocation.chars().count() <= MAX_INVOCATION_DISPLAY {
        return invocation.to_string();
    }
    // Count in chars, not bytes, so this can't split a multi-byte character.
    invocation
        .chars()
        .take(MAX_INVOCATION_DISPLAY - 1)
        .chain(['…'])
        .collect()
}

/// Build the final report: a short status line, plus logs attached as files
/// rather than pasted inline — SteamCMD's output routinely exceeds Discord's
/// 2000-character message limit, and a file keeps the whole log instead of a
/// lossy truncation. `console_output` (see `console_log_path`) is attached
/// whenever this run produced any, regardless of outcome, since it's the more
/// reliable record of what actually happened.
fn render_result(
    args: &[&str],
    outcome: Outcome,
    console_output: Vec<u8>,
) -> serenity::CreateMessage {
    let invocation = truncate_invocation(&args.join(" "));

    let mut message = match outcome {
        Outcome::TimedOut => serenity::CreateMessage::new().content(format!(
            "SteamCMD (`{}`) did not finish and was killed after timing out.",
            invocation
        )),
        Outcome::FailedToStart(e) => serenity::CreateMessage::new().content(format!(
            "Could not start SteamCMD (`{}`): {}",
            invocation, e
        )),
        Outcome::Ran(output) => {
            let status = match output.status.code() {
                Some(0) => "finished successfully".to_string(),
                Some(code) => format!("exited with code {}", code),
                None => "was terminated by a signal".to_string(),
            };

            let mut message = serenity::CreateMessage::new()
                .content(format!("SteamCMD (`{}`) {}.", invocation, status));

            if !output.stdout.is_empty() {
                message = message.add_file(serenity::CreateAttachment::bytes(
                    output.stdout,
                    "stdout.log",
                ));
            }
            if !output.stderr.is_empty() {
                message = message.add_file(serenity::CreateAttachment::bytes(
                    output.stderr,
                    "stderr.log",
                ));
            }
            message
        }
    };

    if !console_output.is_empty() {
        message = message.add_file(serenity::CreateAttachment::bytes(
            console_output,
            "steamcmd_console.log",
        ));
    }

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_reason_flags_missing_quit() {
        assert!(invalid_reason(&[]).is_some());
        assert!(invalid_reason(&["+login", "anonymous"]).is_some());
        assert!(invalid_reason(&["+login", "anonymous", "+quit"]).is_none());
    }

    #[test]
    fn validate_executable_accepts_a_real_file() {
        let tmp = std::env::temp_dir().join(format!("update_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let exe = tmp.join("steamcmd.exe");
        std::fs::write(&exe, b"x").unwrap();

        assert_eq!(validate_executable(exe.clone()).unwrap(), exe);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn console_log_path_sits_in_logs_next_to_the_exe() {
        let exe = Path::new("C:/Games/steamcmd/steamcmd.exe");
        assert_eq!(
            console_log_path(exe),
            Path::new("C:/Games/steamcmd/logs/console_log.txt")
        );
    }

    #[test]
    fn file_len_is_zero_for_a_missing_file() {
        let missing = std::env::temp_dir().join("definitely_not_here.txt");
        assert_eq!(file_len(&missing), 0);
    }

    #[test]
    fn read_appended_returns_only_what_was_added_after_the_offset() {
        let path = std::env::temp_dir().join(format!("update_test_log_{}.txt", std::process::id()));
        std::fs::write(&path, b"from a previous run\n").unwrap();
        let offset = file_len(&path);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        use std::io::Write;
        file.write_all(b"from this run\n").unwrap();
        drop(file);

        assert_eq!(read_appended(&path, offset), b"from this run\n");

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn read_appended_is_empty_for_a_missing_file() {
        let missing = std::env::temp_dir().join("definitely_not_here_either.txt");
        assert!(read_appended(&missing, 0).is_empty());
    }

    #[test]
    fn validate_executable_rejects_a_missing_path() {
        let missing = std::env::temp_dir().join("definitely_not_here_steamcmd.exe");
        assert!(validate_executable(missing).is_err());
    }

    #[test]
    fn validate_executable_rejects_a_directory() {
        assert!(validate_executable(std::env::temp_dir()).is_err());
    }
}
