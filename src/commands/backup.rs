use directories::UserDirs;
use lnk::ShellLink;
use poise::serenity_prelude as serenity;
use std::fs;

use crate::{Context, Error};

/// Post every existing save zip from the save folder shortcut.
#[poise::command(prefix_command, slash_command)]
pub async fn backup(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(user_dirs) = UserDirs::new() {
        let save_dir = user_dirs.desktop_dir().unwrap().join("forest_saves.lnk");

        let deref = match ShellLink::open(save_dir) {
            Err(_) => {
                ctx.say("Save folder not found.").await?;
                return Err(Error::from("File not found."));
            }
            Ok(f) => f,
        };

        let mut paths = fs::read_dir(deref.relative_path().as_ref().unwrap())?
            .filter(|p| {
                p.as_ref()
                    .unwrap()
                    .path()
                    .to_str()
                    .unwrap()
                    .contains(".zip")
            })
            .collect::<Vec<_>>();
        println!("{:?}", paths);
        paths.sort_by(|a, b| {
            a.as_ref()
                .unwrap()
                .metadata()
                .unwrap()
                .created()
                .unwrap()
                .cmp(&b.as_ref().unwrap().metadata().unwrap().created().unwrap())
        });
        for backup in &paths {
            let path = backup.as_ref().clone().unwrap().path();
            let path_str = String::from(path.clone().to_str().unwrap());
            let attachment = serenity::CreateAttachment::path(&path).await?;
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("{:?}", path_str))
                    .attachment(attachment),
            )
            .await?;
        }
    }
    Ok(())
}
