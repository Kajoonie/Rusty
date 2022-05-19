use std::thread;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use futures::executor::block_on;
use redis::Commands;
use serenity::{client::Context, framework::standard::{
    Args, CommandResult,
    macros::command,
}, model::channel::Message};
use serenity::model::user::User;

use crate::redis_ip;

struct Timer {
    name: String,
    time: Duration,
    mentions: Vec<User>,
}

#[command]
#[aliases("time", "remind", "reminder")]
#[sub_commands("timezone")]
#[description = "Start a timer (ex. '!timer zombie murder simulator at 8:30pm @Rusty')"]
async fn timer(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let arg_str = String::from(args.rest());

    let t = get_timer_from_args(msg.clone(), arg_str).await;

    if let Some(timer) = t {
        println!("Timer -- Name: {}, Duration: {:?}", timer.name, timer.time);

        let duration = &timer.time;

        let seconds = duration.as_secs() % 60;
        let minutes = (duration.as_secs() / 60) % 60;
        let hours = (duration.as_secs() / 60) / 60;
        let result = msg.channel_id.send_message(&ctx.http, |m| m
            .content(format!("Okay, I've set a timer for **{:0>2}:{:0>2}:{:0>2}**.", hours, minutes, seconds)),
        ).await;

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        let ctx_clone = ctx.clone();
        let msg_clone = msg.clone();

        thread::spawn(move || block_on(notify_on_expiry(ctx_clone, msg_clone, timer)));

        Ok(())
    } else {
        let result = msg.reply(&ctx.http, "Sorry, I couldn't determine a time from your message. Have you set your timezone? Try \"!timer tz America/New_York\"").await;

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
        for j in (i..len + 1).rev() {
            if j < len && !arg_str.chars().nth(j).unwrap().is_whitespace() {
                continue;
            }
            let slice_duration = parse_duration(&msg, &arg_str[i..j]);
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

fn parse_duration(msg: &Message, time: &str) -> Option<Duration> {
    let parse_relative_result = parse_relative_duration(time);

    if let None = parse_relative_result {
        parse_explicit_duration(msg, time)
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
        }
    }
}

fn parse_explicit_duration(msg: &Message, time: &str) -> Option<Duration> {
    let timer_stop_parsed = match two_timer::parse(time, None) {
        Ok((d1, _, _)) => d1,
        Err(_e) => {
            //println!("Failed to parse duration from {}: {:?}", time, e);
            NaiveDateTime::from_timestamp(0, 0)
        }
    };

    let redis = redis::Client::open(redis_ip()).ok()?;
    let mut con = redis.get_connection().ok()?;

    let user_timezone: Option<String> = con.get(&msg.author.to_string()).ok()?;

    if let Some(tz_str) = user_timezone {
        let tz: Tz = match tz_str.parse() {
            Ok(timezone) => timezone,
            Err(_) => Tz::UTC,
        };

        let utc_naive = Utc::now().naive_utc();
        let tz_time: DateTime<Tz> = tz.from_utc_datetime(&utc_naive);
        let utc_offset = tz_time.offset().fix().local_minus_utc() as i64;

        let timer_stop_ms = timer_stop_parsed.timestamp_millis() - (utc_offset * 1000);

        let res = timer_stop_ms - tz_time.timestamp_millis();

        if res < 0 {
            /* For some reason typical time statements such as "8:30pm" are interpreted as the most
            recent _past_ occurrence of that time, rather than the closest future occurrence.
            As such, we'll try to add one days worth of milliseconds to the result to see if we get
            a valid duration from it.
            This means someone intentionally trying to set a timer for a point in time "yesterday"
            will instead set a timer for today's equivalent time instead of failing. */
            let res_2 = timer_stop_ms + (1000 * 60 * 60 * 24) - tz_time.timestamp_millis();
            if res_2 >= 0 {
                return Some(Duration::from_millis(res_2 as u64));
            }
        } else {
            return Some(Duration::from_millis(res as u64));
        }
    }

    None
}

async fn notify_on_expiry(ctx: Context, msg: Message, timer: Timer) {
    thread::sleep(timer.time);
    let mut mentions = String::new();
    timer.mentions.iter().for_each(|f| {
        mentions += &*format!("{}", f);
    });
    let result = msg.reply(ctx.http, format!("Your timer has expired.\n{}", mentions)).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }
}

#[command]
#[aliases("tz")]
async fn timezone(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let redis = redis::Client::open(redis_ip())?;
    let mut con = redis.get_connection()?;

    if args.len() == 0 {
        let current_tz: Option<String> = con.get(&msg.author.to_string())?;
        let result;
        if let Some(tz) = current_tz {
            result = msg.reply(&ctx.http, format!("Your timezone is currently set to {}", tz)).await;
        } else {
            result = msg.reply(&ctx.http, format!("Your timezone has not yet been set!")).await;
        }

        if let Err(why) = result {
            println!("Unable to send message: {:?}", why);
        }

        return Ok(());
    }

    let arg_str = args.rest();
    let tz: Tz = match arg_str.parse() {
        Ok(timezone) => timezone,
        Err(_) => Tz::UTC,
    };

    con.set(&msg.author.to_string(), tz.to_string())?;

    let result = msg.reply(&ctx.http, format!("Your timezone has been set to {}", tz)).await;

    if let Err(why) = result {
        println!("Unable to send message: {:?}", why);
    }

    Ok(())
}