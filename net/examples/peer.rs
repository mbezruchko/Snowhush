//! A friend for trying a room out alone: hosts a room or joins one, says what happens, throws at someone in it every
//! so often and back at whoever hits it.
//!
//!     cargo run --example peer -- host [name]
//!     cargo run --example peer -- join <invite> [name]

use std::time::{Duration, SystemTime};

use snowhush_net::{Event, host, join};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let relay = std::env::var("SNOWHUSH_RELAY").ok();
    let room = match args.next().as_deref() {
        Some("host") => host(args.next().unwrap_or_else(|| "Peer".into()), relay).await?,
        Some("join") => {
            let ticket = args.next().expect("an invite");
            join(ticket, args.next().unwrap_or_else(|| "Peer".into()), relay).await?
        }
        _ => panic!("host [name] | join <invite> [name]"),
    };
    let mut others: Vec<String> = vec![];
    let mut me = String::new();
    let mut next_throw = tokio::time::Instant::now() + Duration::from_secs(6);
    let mut answer: Option<(tokio::time::Instant, String)> = None;
    loop {
        while let Some(event) = room.next() {
            println!("{event:?}");
            match &event {
                Event::Room { ticket, me: id } => {
                    me = id.clone();
                    println!("invite: snowhush://join/{ticket}");
                }
                Event::Joined { me: id } => me = id.clone(),
                Event::Player { id, .. } => others.push(id.clone()),
                Event::Left { id } => others.retain(|o| o != id),
                Event::Throw { from, at } if *at == me => answer = Some((tokio::time::Instant::now() + Duration::from_millis(1500), from.clone())),
                Event::Closed { .. } => return Ok(()),
                _ => {}
            }
        }
        let now = tokio::time::Instant::now();
        if let Some((when, at)) = answer.clone() {
            if now >= when {
                println!("throwing back at {at}");
                room.throw(&at);
                answer = None;
            }
        }
        if now >= next_throw {
            if let Some(target) = others.last() {
                println!("throwing at {target}");
                room.throw(target);
            }
            let jitter = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_millis() as u64 % 6).unwrap_or(0);
            next_throw = now + Duration::from_secs(8 + jitter);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
