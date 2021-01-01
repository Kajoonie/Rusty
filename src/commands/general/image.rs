use serenity::{
    framework::standard::{
        CommandResult, Args,
        macros::command,
    },
    model::channel::Message,
    client::Context,
};
use std::{env, fs};

use strsim::levenshtein;
use tokio::fs::File;
use std::ffi::OsString;
use std::path::{PathBuf, Path};
use tokio::io::AsyncWriteExt;

#[command]
#[only_in(guilds)]
#[sub_commands(list, add, remove, update)]
#[aliases("img", "images")]
#[description = "Post images by name! Add, remove, or update them as you please"]
async fn image(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut path = env::current_dir()?.join("src/img");

    let image_name = args.single::<OsString>()?;

    if find_exact_file_match(&mut path, &image_name).unwrap() {
        return send_attachment(ctx, msg, &path).await;
    }

    let close_matches = get_similar_files(&path, &image_name).ok_or("Failed finding similar files")?;

    if !response_for_zero_or_multiple_similar_files(&ctx, &msg, &close_matches).await.ok_or("Unable to send response")? {
        let matched_path = close_matches.get(0);

        if let Some(attachment) = matched_path {
            return send_attachment(ctx, msg, attachment).await;
        }
    }

    Ok(())
}

fn find_exact_file_match(path: &mut PathBuf, image_name: &OsString) -> Option<bool> {
    for entry in fs::read_dir(&path).ok()? {
        let entry = entry.ok()?;

        if let Some(stem) = entry.path().file_stem() {
            if stem.eq(image_name) {
                path.push(entry.path());
                return Option::from(true);
            }
        }
    }

    return Option::from(false);
}

async fn send_attachment(ctx: &Context, msg: &Message, path: &PathBuf) -> CommandResult {
    let f1 = File::open(&path).await?;

    if let Some(file_name) = path.file_name() {
        if let Some(file_name_str) = file_name.to_str() {

            let files = vec![(&f1, file_name_str)];
            let result = msg.channel_id.send_files(&ctx.http, files, |m| m).await;

            if let Err(why) = result {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    Ok(())
}

fn get_similar_files(path: &PathBuf, image_name: &OsString) -> Option<Vec<PathBuf>> {
    let mut close_matches = vec![];

    let image_name_str = image_name.clone().into_string().ok()?;

    for entry in fs::read_dir(&path).ok()? {
        let entry = entry.ok()?;
        if let Some(file_stem) = entry.path().file_stem() {
            if let Some(file_stem_str) = file_stem.to_str() {
                if levenshtein(&image_name_str, file_stem_str) < 3 {
                    close_matches.push(entry.path());
                }
            }
        }
    }

    return Option::from(close_matches);
}

async fn response_for_zero_or_multiple_similar_files(ctx: &Context, msg: &Message, close_matches: &Vec<PathBuf>) -> Option<bool> {
    // Case 1 : Multiple similar files found
    if close_matches.len() > 1 {
        let close_filenames: Vec<_> = close_matches.iter().map(|f| f
            .file_stem()
            .unwrap()
        ).collect();

        let response = format!("File not found. Did you mean one of these? {:?}", close_filenames);

        let result = msg.reply(&ctx.http, response).await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        return Option::from(true);
    }

    // Case 2 : No similar files found
    if close_matches.len() == 0 {
        let result = msg.reply(&ctx.http, "No file with that name was found").await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        return Option::from(true);
    }

    return Option::from(false);
}

struct Image {
    name_with_ext: OsString,
    path: PathBuf,
    bytes: Vec<u8>,
}

async fn get_image_from_message(msg: &Message, mut args: Args) -> Option<Image> {
    let attachment = msg.attachments.get(0);

    if let Some(image_attachment) = attachment {
        let name = args.single::<OsString>().ok()?;
        let ext_opt = Path::new(&image_attachment.filename).extension();

        if let Some(ext) = ext_opt {
            let mut filename = OsString::new();
            filename.push(&name);
            filename.push(".");
            filename.push(&ext);

            let bytes = image_attachment.download().await.ok()?;

            let path = env::current_dir().ok()?.join("src/img").join(&filename);

            return Some(Image {
                name_with_ext: filename,
                path,
                bytes,
            });
        }
    }

    None
}

async fn save_image_with_response(ctx: &Context, msg: &Message, img: &Image) -> CommandResult {
    let mut file = File::create(&img.path).await?;

    let file_write_result = file.write_all(&img.bytes).await;

    let msg_send_result;

    if file_write_result.is_ok() {
        msg_send_result = msg.reply(&ctx.http, "Image successfully saved.").await;
    } else {
        msg_send_result = msg.reply(&ctx.http, "Something went wrong while attempting to save that image.").await;
    }

    if let Err(why) = msg_send_result {
        println!("Error sending message: {:?}", why);
    }

    if let Err(why) = file_write_result {
        println!("Error saving image: {:?}", why);
    }

    Ok(())
}

#[command]
async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let image_opt = get_image_from_message(&msg, args).await;

    if let Some(image) = image_opt {
        if !image.path.exists() {
            let result = save_image_with_response(&ctx, &msg, &image).await;

            if let Err(why) = result {
                println!("Unable to send message: {:?}", why);
            }
        } else {
            let result = msg.reply(&ctx.http, format!("An image named {:?} already exists", image.name_with_ext)).await;

            if let Err(why) = result {
                println!("Unable to send message: {:?}", why);
            }
        }
    } else {
        let result = msg.reply(&ctx.http, "You've gotta include an image.").await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }
    }

    Ok(())
}

#[command]
async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {

    let name = args.single::<OsString>()?;
    let mut path = env::current_dir()?.join("src/img");

    let msg_send_result;

    let file_match = find_exact_file_match(&mut path, &name).unwrap();

    if file_match {
        let result = fs::remove_file(&path);

        if let Err(why) = &result {
            println!("Error removing file: {:?}", why);
        }

        if result.is_ok() {
            msg_send_result = msg.reply(&ctx.http, "Image successfully deleted.").await;
        } else {
            msg_send_result = msg.reply(&ctx.http, "Something went wrong while attempting to delete that image.").await;
        }

        if let Err(why) = msg_send_result {
            println!("Error sending message: {:?}", why);
        }

    } else {
        let close_matches_opt = get_similar_files(&path, &name);

        if let Some(close_matches) = close_matches_opt {

            let response_sent_opt = response_for_zero_or_multiple_similar_files(&ctx, &msg, &close_matches).await;

            if let Some(response_sent) = response_sent_opt {
                if !response_sent {
                    let single_match_path_opt = close_matches.get(0);
                    if let Some(single_match_path) = single_match_path_opt {
                        return send_attachment(ctx, msg, single_match_path).await;
                    }
                }
            }
        }
    }

    Ok(())
}

#[command]
async fn update(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let image_opt = get_image_from_message(&msg, args).await;

    if let Some(image) = image_opt {
        if image.path.exists() {
            let result = save_image_with_response(&ctx, &msg, &image).await;

            if let Err(why) = result {
                println!("Unable to send message: {:?}", why);
            }
        } else {
            let result = msg.reply(&ctx.http, format!("No image named {:?} exists.", &image.name_with_ext)).await;

            if let Err(why) = result {
                println!("Unable to send message: {:?}", why);
            }
        }
    }

    Ok(())
}

#[command]
async fn list(ctx: &Context, msg: &Message) -> CommandResult {

    let path = env::current_dir()?.join("src/img");

    let mut files = vec![];

    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        files.push(entry.path());
    }

    let filenames: Vec<_> = files.iter().map(|f| f
        .file_stem()
        .unwrap()
    ).collect();

    let response = format!("Here's what we've got in stock: {:?}", filenames);

    let result = msg.reply(&ctx.http, response).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }

    Ok(())
}


