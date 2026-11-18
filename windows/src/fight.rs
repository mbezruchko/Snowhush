//! The snowball fight's room as this app plays in it, as the Mac's SnowballRoom does: opened or joined through the net
//! crate, with the relay and whether playing online is on as snowhush.app/relay.json says, the invite link on the
//! clipboard, and what the network says happened handed to the room. The snow's loop plays; the menu shows the room as
//! it last said.

use std::sync::{Mutex, mpsc};

use snowhush_core::snowman::room::{self, Failure, Heard, Room, State};
use snowhush_core::text::tr;
use snowhush_net::{Event, play};
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::PWSTR;

use crate::log;
use crate::wallpaper;

/// The room as the menu shows it: what is going on, whether hosting it, and whether in one at all.
static SHOWN: Mutex<(Option<String>, bool, bool)> = Mutex::new((None, false, false));

/// The room being opened or joined, waiting on what the site says.
pub struct Opening(mpsc::Receiver<(Option<String>, bool)>, Option<String>);

/// What friends see this player called: the first of the account's names.
fn name() -> String {
    let mut buffer = [0u16; 256];
    let mut size = buffer.len() as u32;
    let full = unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buffer.as_mut_ptr())), &mut size) }
        .then(|| String::from_utf16_lossy(&buffer[..size as usize]))
        .filter(|full| !full.trim().is_empty())
        .unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_default());
    full.split_whitespace().next().filter(|first| !first.is_empty()).unwrap_or("Snowman").to_string()
}

/// The relay to go through and whether playing online is on, as the site says; iroh's own relays and on when it can't
/// be asked.
fn settings() -> (Option<String>, bool) {
    if let Ok(own) = std::env::var("SNOWHUSH_RELAY") {
        return (Some(own), true);
    }
    let said = wallpaper::download("snowhush.app", "/relay.json").ok().and_then(|body| serde_json::from_slice::<serde_json::Value>(&body).ok());
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
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let target = GlobalLock(memory);
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), target.cast::<u16>(), wide.len());
                let _ = GlobalUnlock(memory);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(memory.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

/// What is on the clipboard, if it is text.
pub fn pasted() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let text = GetClipboardData(CF_UNICODETEXT.0 as u32).ok().and_then(|data| {
            let memory = HGLOBAL(data.0);
            let source = GlobalLock(memory).cast::<u16>();
            if source.is_null() {
                return None;
            }
            let length = (0..).take_while(|&k| *source.add(k) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(source, length));
            let _ = GlobalUnlock(memory);
            Some(text)
        });
        let _ = CloseClipboard();
        text
    }
}

/// Keeps the room as the menu is to show it.
pub fn show(room: &Room) {
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = (status(room), room.invite.is_some(), room.state != State::None);
    }
}

/// The room as the menu shows it.
pub fn shown() -> (Option<String>, bool, bool) {
    SHOWN.lock().map(|shown| shown.clone()).unwrap_or_default()
}

/// What is going on in the room, or none while there is none.
fn status(room: &Room) -> Option<String> {
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
