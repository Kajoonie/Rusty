use serenity::{framework::standard::{
    CommandResult, Args,
    macros::command,
}, model::channel::Message, client::Context};
use std::thread;
use std::time::Duration;
use futures::executor::block_on;
use chrono::{NaiveDateTime, Local, TimeZone, Offset};

struct Timer<'a> {
    name: &'a str,
    time: &'a str,
}

#[command]
#[aliases("time", "remind", "reminder")]
#[description = "Start a timer (ex. '!timer name: making coffee time: 5 minutes' or '!timer 30 minutes until game time)"]
async fn timer(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    let arg_str = args.rest();

    let timer = parse_name_and_duration_from_args(&ctx, &msg, &arg_str).await.unwrap();

    let name = timer.name;
    let time = timer.time;

    let mut duration = attempt_parse_duration(time);

    if duration.as_secs() == 0 {
        duration = attempt_two_timer_parse(time);
        if duration.as_secs() == 0 {
            let result = msg.reply(&ctx.http, format!("Sorry, I couldn't understand what you meant by **{}**.", time)).await;

            if let Err(why) = result {
                println!("Unable to send message: {:?}", why);
            }

            return Ok(());
        }
    }

    let seconds = duration.as_secs() % 60;
    let minutes = (duration.as_secs() / 60) % 60;
    let hours = (duration.as_secs() / 60) / 60;
    let result = msg.reply(&ctx.http, format!("Okay, I'll alert you in **{:0>2}:{:0>2}:{:0>2}**.", hours, minutes, seconds)).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }

    let ctx_clone = ctx.clone();
    let msg_clone = msg.clone();
    let name_clone = String::new() + name;

    thread::spawn(move || block_on(notify_on_expiry(ctx_clone, msg_clone, duration, name_clone)));

    Ok(())
}

async fn parse_name_and_duration_from_args<'a>(ctx: &Context, msg: &Message, arg_str: &'a str) -> Option<Timer<'a>> {
    let name_label = arg_str.find("name:");
    if name_label.is_none() {
        let result = msg.reply(&ctx.http, "Please include a \"name:\" label for your timer.").await;

        if let Err(why) = result {
            println!("Unable to send message {:?}", why);
        }

        return None;
    }

    let time_label = arg_str.find("time:");
    if time_label.is_none() {
        let result = msg.reply(&ctx.http, "Please include a \"time:\" label for your timer.").await;

        if let Err(why) = result {
            println!("Unable to send message {:?}", why);
        }

        return None;
    }

    let name_label_start = name_label.unwrap();
    let time_label_start = time_label.unwrap();

    let name;
    let time;

    if name_label_start < time_label_start {
        name = arg_str[name_label_start+5..time_label_start].trim();
        time = arg_str[time_label_start+5..].trim();
    } else {
        time = arg_str[time_label_start+5..name_label_start].trim();
        name = arg_str[name_label_start+5..].trim();
    }

    Some(Timer {name, time})
}

fn attempt_parse_duration(time: &str) -> Duration {
    match parse_duration::parse(time) {
        Ok(duration) => duration,
        Err(e) => {
            println!("Failed to parse duration from {}: {:?}", time, e);
            Duration::new(0, 0)
        },
    }
}

fn attempt_two_timer_parse(time: &str) -> Duration {
    let timer_stop_parsed = match two_timer::parse(time, None) {
        Ok((d1, _, _)) => d1,
        Err(e) => {
            println!("Failed to parse duration from {}: {:?}", time, e);
            NaiveDateTime::from_timestamp(0, 0)
        },
    };

    let utc_offset = Local.timestamp(0, 0).offset().fix().local_minus_utc() as i64;

    println!("Understood {} as {:?}", &time, &timer_stop_parsed);

    let timer_stop_ms = timer_stop_parsed.timestamp_millis() - (utc_offset * 1000);
    println!("timer_stop_parsed in ms is {}", timer_stop_ms);

    println!("UTC Offset is {}", utc_offset);
    let current_ms = chrono::offset::Local::now().timestamp_millis();
    println!("current_ms is {}", current_ms);

    let res = timer_stop_ms - current_ms;

    if res < 0 {
        // For some reason typical time statements such as "8:30pm" are interpreted as the most
        // recent occurrence of that time in the past, rather than the closest future occurrence.
        // As such, we'll try to add one days worth of milliseconds to the result to see if we get
        // a valid duration from it.
        let res_2 = timer_stop_ms + (1000 * 60 * 60 * 24) - current_ms;
        if res_2 < 0 {
            Duration::new(0, 0)
        } else {
            Duration::from_millis(res_2 as u64)
        }
    } else {
        Duration::from_millis(res as u64)
    }
}

async fn notify_on_expiry(ctx: Context, msg: Message, delay: Duration, name: String) {
    thread::sleep(delay);
    let result = msg.reply_mention(ctx.http, format!("Your **{}** timer is up!", name)).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }
}