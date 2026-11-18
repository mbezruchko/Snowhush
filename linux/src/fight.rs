//! The snowball fight's room as this app plays in it, as the Mac's SnowballRoom does: opened or joined through the net
//! crate, with the relay and whether playing online is on as snowhush.app/relay.json says, the invite link on the
//! clipboard, and what the network says happened handed to the room.

use std::process::{Command, Stdio};
use std::sync::mpsc;

use snowhush_core::snowman::room::{self, Failure, Heard, Room, State};
use snowhush_core::text::tr;
use snowhush_net::{Event, play};

use crate::log;

/// What snowhush.app says about playing online, asked on opening or joining a room.
const SETTINGS: &str = "https://snowhush.app/relay.json";

/// The room being opened or joined, waiting on what the site says.
pub struct Opening(mpsc::Receiver<(Option<String>, bool)>, Option<String>);

/// What friends see this player called: the first of the account's names.
fn name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let full = Command::new("getent")
        .args(["passwd", &user])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|line| line.split(':').nth(4).map(|gecos| gecos.split(',').next().unwrap_or_default().to_string()))
        .filter(|full| !full.trim().is_empty());
    let name = full.unwrap_or(user);
    name.split_whitespace().next().filter(|first| !first.is_empty()).unwrap_or("Snowman").to_string()
}

/// The relay to go through and whether playing online is on, as the site says, three seconds at most; iroh's own relays
/// and on when it can't be asked.
fn settings() -> (Option<String>, bool) {
    if let Ok(own) = std::env::var("SNOWHUSH_RELAY") {
        return (Some(own), true);
    }
    let said = Command::new("curl")
        .args(["--fail", "--silent", "--location", "--max-time", "3", SETTINGS])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| serde_json::from_slice::<serde_json::Value>(&output.stdout).ok());
    let relay = said.as_ref().and_then(|said| said["relay"].as_str()).filter(|relay| relay.starts_with("https://")).map(str::to_string);
    let online = said.as_ref().and_then(|said| said["online"].as_bool()).unwrap_or(true);
    (relay, online)
}

/// Opens a room, or joins the one of `invite`.
pub fn open(room: &mut Room, invite: Option<String>) -> Opening {
    room.opening(invite.is_some());
    let (sender, said) = mpsc::channel();
    let _ = std::thread::Builder::new().name("fight".into()).spawn(move || {
        let _ = sender.send(settings());
    });
    Opening(said, invite)
}

/// Goes on opening or joining once the site has said whether to; returns whether it is past that.
pub fn go_on(opening: &Opening, room: &mut Room) -> bool {
    let Ok((relay, online)) = opening.0.try_recv() else { return false };
    if !online {
        room.fail(Failure::Unavailable);
        return true;
    }
    let name = name();
    match &opening.1 {
        Some(invite) => play::join(invite.trim().trim_start_matches("snowhush://join/"), &name, relay.as_deref()),
        None => play::host(&name, relay.as_deref()),
    }
    true
}

/// Hands what the network said happened to the room. Returns the throws others made that counted, `from` and `at`.
pub fn listen(room: &mut Room, now: f64) -> Vec<(String, String)> {
    let mut thrown = Vec::new();
    while let Some(event) = play::next() {
        let heard = match event {
            Event::Room { ticket, me } => {
                copy(&room::link(&ticket));
                Heard::Room { ticket, me }
            }
            Event::Joined { me } => Heard::Joined { me },
            Event::Player { id, name } => Heard::Player { id, name },
            Event::Left { id } => Heard::Left { id },
            Event::Throw { from, at } => Heard::Throw { from, at },
            Event::Closed { reason } => {
                play::leave();
                Heard::Closed { full: reason == "full" }
            }
            Event::Error { message } => {
                log!("fight: {message}");
                play::leave();
                Heard::Error
            }
        };
        thrown.extend(room.take(heard, now));
    }
    thrown
}

pub fn leave(room: &mut Room) {
    play::leave();
    room.leave();
}

/// Puts text on the clipboard.
pub fn copy(text: &str) {
    let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        log!("fight: no wl-copy to put the invite on the clipboard: {text}");
        return;
    };
    if let Some(mut input) = child.stdin.take() {
        let _ = std::io::Write::write_all(&mut input, text.as_bytes());
    }
    let _ = child.wait();
}

/// What is on the clipboard, if it is text: a second at most, as the clipboard's owner may never answer.
pub fn pasted() -> Option<String> {
    crate::wallpaper::run("wl-paste", &["--no-newline", "--type", "text"])
}

/// The room as the menu shows it: what is going on, or none while there is no room.
pub fn status(room: &Room) -> Option<String> {
    let name = |id: &str| if Some(id) == room.me() { tr("fight.you").to_string() } else { room.name(id).unwrap_or_default().to_string() };
    match &room.state {
        State::None => None,
        State::Opening => Some(tr("fight.opening").to_string()),
        State::Joining => Some(tr("fight.joining").to_string()),
        State::Open { me } => Some(match &room.winner {
            Some(winner) if winner == me => tr("fight.youwon").to_string(),
            Some(winner) => tr("fight.won").replace("%@", &name(winner)),
            None if room.players.is_empty() => tr("fight.waiting").to_string(),
            None => std::iter::once(me.as_str())
                .chain(room.players.iter().map(|player| player.id.as_str()))
                .map(|id| format!("{} ♥{}", name(id), room.hearts(id)))
                .collect::<Vec<_>>()
                .join(" · "),
        }),
        State::Failed(failure) => Some(
            tr(match failure {
                Failure::Unavailable => "fight.unavailable",
                Failure::Full => "fight.full",
                Failure::Closed => "fight.closed",
                Failure::Failed => "fight.failed",
            })
            .to_string(),
        ),
    }
}
