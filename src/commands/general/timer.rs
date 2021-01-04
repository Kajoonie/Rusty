use serenity::{framework::standard::{
    CommandResult, Args,
    macros::command,
}, model::channel::Message, client::Context};
use std::thread;
use std::time::Duration;
use futures::executor::block_on;
use chrono::{NaiveDateTime, Local, TimeZone, Offset};
use serenity::model::user::User;

struct Timer {
    name: String,
    time: Duration,
    mentions: Vec<User>,
}

#[command]
#[aliases("time", "remind", "reminder")]
#[description = "Start a timer (ex. '!timer name: making coffee time: 5 minutes' or '!timer 30 minutes until game time)"]
async fn timer(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let arg_str = String::new() + args.rest();

    let t = get_timer_from_args(msg.clone(), arg_str).await;

    if let Some(timer) = t {
        println!("Timer -- Name: {}, Duration: {:?}", timer.name, timer.time);

        let duration = &timer.time;

        let seconds = duration.as_secs() % 60;
        let minutes = (duration.as_secs() / 60) % 60;
        let hours = (duration.as_secs() / 60) / 60;
        let result = msg.channel_id.send_message(&ctx.http, |m| m
            .content(format!("Okay, I'll alert you in **{:0>2}:{:0>2}:{:0>2}**.", hours, minutes, seconds))
        ).await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        let ctx_clone = ctx.clone();
        let msg_clone = msg.clone();

        thread::spawn(move || block_on(notify_on_expiry(ctx_clone, msg_clone, timer)));

        Ok(())
    } else {
        let result = msg.reply(&ctx.http, "Sorry, I couldn't determine a time from your message.").await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        Ok(())
    }
}

async fn get_timer_from_args(msg: Message, arg_str: String) -> Option<Timer> {
    let len = arg_str.len();

    for i in 0..len {
        if i > 0 && !arg_str.chars().nth(i).unwrap().is_whitespace() {
            continue;
        }
        for j in (i..len).rev() {
            if j < len && !arg_str.chars().nth(j).unwrap().is_whitespace() {
                continue;
            }
            let slice_duration = parse_duration(&arg_str[i..j]);
            if let Some(time) = slice_duration {
                println!("Got a valid duration of {:?} from the string {:?}", time, &arg_str[i..j]);

                return Some(Timer {
                    name: arg_str,
                    time,
                    mentions: msg.clone().mentions,
                });
            }
        }
    }

    None
}

fn parse_duration(time: &str) -> Option<Duration> {
    let parse_relative_result = parse_relative_duration(time);

    if let None = parse_relative_result {
        parse_explicit_duration(time)
    } else {
        parse_relative_result
    }

}

fn parse_relative_duration(time: &str) -> Option<Duration> {
    match parse_duration::parse(time) {
        Ok(duration) => Some(duration),
        Err(_e) => {
            //println!("Failed to parse duration from {}: {:?}", time, e);
            None
        },
    }
}

fn parse_explicit_duration(time: &str) -> Option<Duration> {
    let timer_stop_parsed = match two_timer::parse(time, None) {
        Ok((d1, _, _)) => d1,
        Err(_e) => {
            //println!("Failed to parse duration from {}: {:?}", time, e);
            NaiveDateTime::from_timestamp(0, 0)
        },
    };

    let utc_offset = Local.timestamp(0, 0).offset().fix().local_minus_utc() as i64;
    let timer_stop_ms = timer_stop_parsed.timestamp_millis() - (utc_offset * 1000);
    let current_ms = chrono::offset::Local::now().timestamp_millis();

    let res = timer_stop_ms - current_ms;

    if res < 0 {
        // For some reason typical time statements such as "8:30pm" are interpreted as the most
        // recent _past_ occurrence of that time, rather than the closest future occurrence.
        // As such, we'll try to add one days worth of milliseconds to the result to see if we get
        // a valid duration from it.
        // This means someone intentionally trying to set a timer for a point in time "yesterday"
        // will instead set a timer for today's equivalent time instead of failing.
        let res_2 = timer_stop_ms + (1000 * 60 * 60 * 24) - current_ms;
        if res_2 < 0 {
            None
        } else {
            Some(Duration::from_millis(res_2 as u64))
        }
    } else {
        Some(Duration::from_millis(res as u64))
    }
}

async fn notify_on_expiry(ctx: Context, msg: Message, timer: Timer) {
    thread::sleep(timer.time);
    let mut mentions = String::new();
    timer.mentions.iter().for_each(|f| {
        mentions += &*format!("{}", f);
    });
    let result = msg.reply(ctx.http, format!("This timer has expired.\n{}", mentions)).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }
}