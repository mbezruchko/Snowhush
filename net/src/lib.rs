//! Rooms for a snowball fight between friends, with no server of ours in between.
//!
//! One player hosts a room and passes on its invite; the others join it with that. Each guest connects straight to
//! the host over iroh (QUIC, through home routers where it can, through a relay where it can't), and the host passes
//! on what each says to the rest. Only what happens goes between them: who is in the room and who threw at whom. Where
//! anyone stands is each one's own: every player places the others in their own wallpaper.
//!
//! The app drives it through a few C functions and reads what happened, one event at a time, as JSON.

use std::{
    collections::{HashMap, VecDeque},
    ffi::{CStr, CString, c_char},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use data_encoding::BASE64URL_NOPAD;
use iroh::{
    Endpoint, EndpointAddr, EndpointId, RelayMap, RelayMode, RelayUrl, SecretKey,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Runtime, sync::mpsc};

/// Tells a Snowhush room from anything else spoken over iroh; the number goes up when what is said changes.
const ALPN: &[u8] = b"snowhush/room/1";
/// The most a single message may be, in bytes: names and ids are short.
const MAX_MESSAGE: usize = 2048;
/// The most players a room holds, the host among them: one invite brings in as many friends as that.
const MAX_PLAYERS: usize = 12;
/// The longest a name may be, in characters.
const MAX_NAME: usize = 24;

/// What the app is told, in order.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    /// Hosting: the room is open, and this is its invite.
    Room {
        ticket: String,
        me: String,
    },
    /// Joined a room as `me`.
    Joined {
        me: String,
    },
    /// Someone is in the room: told of each one already there on joining, and of each who comes after.
    Player {
        id: String,
        name: String,
    },
    Left {
        id: String,
    },
    /// `from` threw a snowball at `at` and hit.
    Throw {
        from: String,
        at: String,
    },
    /// The room is gone: the host left, or the way to it was lost.
    Closed {
        reason: String,
    },
    Error {
        message: String,
    },
}

/// What goes between the players.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Wire {
    Hello { name: String },
    Player { id: String, name: String },
    Left { id: String },
    Throw { from: String, at: String },
}

/// Hosting: a guest's name, the way to send to it, and which of its connections that is.
struct Guest {
    name: String,
    tx: mpsc::UnboundedSender<Wire>,
    connection: Connection,
}

pub struct Room {
    endpoint: Endpoint,
    me: EndpointId,
    name: String,
    events: Mutex<VecDeque<Event>>,
    guests: Mutex<HashMap<EndpointId, Guest>>,
    /// A guest: the way to send to the host.
    host: Mutex<Option<mpsc::UnboundedSender<Wire>>>,
}

impl Room {
    fn tell(&self, event: Event) {
        self.events.lock().unwrap().push_back(event);
    }

    /// Hosting: sends to every guest but `except`.
    fn pass_on(&self, wire: &Wire, except: Option<EndpointId>) {
        for (id, guest) in self.guests.lock().unwrap().iter() {
            if Some(*id) != except {
                let _ = guest.tx.send(wire.clone());
            }
        }
    }
}

static ROOM: Mutex<Option<Arc<Room>>> = Mutex::new(None);
/// Events from before a room was made or after it was left, such as a failure to make one.
static LOOSE: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("snowhush-net")
            .enable_all()
            .build()
            .expect("a runtime for the network")
    })
}

fn tell_loose(event: Event) {
    LOOSE.lock().unwrap().push_back(event);
}

/// The key each player of this app joined a host's room with, by host and name: coming back into a room, after the
/// Mac slept or the way to it was lost, a player is the one he was, not another beside his own ghost. Only for as long
/// as the app runs.
static JOINED_AS: Mutex<Option<HashMap<(EndpointId, String), SecretKey>>> = Mutex::new(None);

/// An endpoint speaking only a room's protocol, as `key` if given; through `relay` when given, else iroh's own relays.
async fn bind(relay: Option<String>, key: Option<SecretKey>) -> Result<Endpoint> {
    let mut builder = Endpoint::builder(presets::N0).alpns(vec![ALPN.to_vec()]);
    if let Some(key) = key {
        builder = builder.secret_key(key);
    }
    if let Some(relay) = relay.filter(|r| !r.is_empty()) {
        let url: RelayUrl = relay.parse().context("the relay's address")?;
        builder = builder.relay_mode(RelayMode::Custom(RelayMap::from(url)));
    }
    Ok(builder.bind().await?)
}

/// An invite short enough to pass on in a message: the host's key, up to three of its IPv4 addresses for a friend on
/// the same network, and its relay, through which the rest of the way is found. Laid out as the key's 32 bytes, the
/// count of addresses, 6 bytes for each, then the relay's address as text.
pub fn encode_ticket(addr: &EndpointAddr) -> Result<String> {
    let mut bytes = addr.id.as_bytes().to_vec();
    let ips: Vec<SocketAddrV4> = addr
        .ip_addrs()
        .filter_map(|a| match a {
            SocketAddr::V4(v4) if !v4.ip().is_loopback() => Some(*v4),
            _ => None,
        })
        .take(3)
        .collect();
    bytes.push(ips.len() as u8);
    for ip in ips {
        bytes.extend_from_slice(&ip.ip().octets());
        bytes.extend_from_slice(&ip.port().to_be_bytes());
    }
    if let Some(relay) = addr.relay_urls().next() {
        bytes.extend_from_slice(relay.to_string().as_bytes());
    }
    Ok(BASE64URL_NOPAD.encode(&bytes))
}

pub fn decode_ticket(ticket: &str) -> Result<EndpointAddr> {
    // As it may come pasted: in a link, with spaces or a line break round it.
    let ticket = ticket.trim().rsplit('/').next().unwrap_or_default();
    let bytes = BASE64URL_NOPAD
        .decode(ticket.as_bytes())
        .map_err(|_| anyhow!("not an invite"))?;
    let invalid = || anyhow!("not an invite");
    let key: &[u8; 32] = bytes
        .get(..32)
        .and_then(|k| k.try_into().ok())
        .ok_or_else(invalid)?;
    let mut addr = EndpointAddr::new(EndpointId::from_bytes(key).map_err(|_| invalid())?);
    let count = *bytes.get(32).ok_or_else(invalid)? as usize;
    let mut at = 33;
    for _ in 0..count {
        let ip = bytes.get(at..at + 6).ok_or_else(invalid)?;
        addr = addr.with_ip_addr(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
            u16::from_be_bytes([ip[4], ip[5]]),
        )));
        at += 6;
    }
    if at < bytes.len() {
        let relay: RelayUrl = std::str::from_utf8(&bytes[at..])
            .context("not an invite")?
            .parse()
            .context("not an invite")?;
        addr = addr.with_relay_url(relay);
    }
    Ok(addr)
}

async fn write(send: &mut SendStream, wire: &Wire) -> Result<()> {
    let bytes = serde_json::to_vec(wire)?;
    if bytes.len() > MAX_MESSAGE {
        bail!("message too long");
    }
    send.write_all(&(bytes.len() as u16).to_be_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

async fn read(recv: &mut RecvStream) -> Result<Wire> {
    let mut length = [0u8; 2];
    recv.read_exact(&mut length).await?;
    let length = u16::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE {
        bail!("message too long");
    }
    let mut bytes = vec![0u8; length];
    recv.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Sends what comes down `rx` until it closes or the stream fails.
fn spawn_writer(mut send: SendStream, mut rx: mpsc::UnboundedReceiver<Wire>) {
    tokio::spawn(async move {
        while let Some(wire) = rx.recv().await {
            if write(&mut send, &wire).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });
}

fn clean(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect();
    let name = name.trim().to_string();
    if name.is_empty() {
        "Snowman".to_string()
    } else {
        name
    }
}

pub async fn host(name: String, relay: Option<String>) -> Result<Arc<Room>> {
    let endpoint = bind(relay, None).await?;
    // The invite carries where the host can be found: wait a little for a relay to be reached, but not for ever, as
    // on one network the addresses alone do.
    let _ = tokio::time::timeout(Duration::from_secs(8), endpoint.online()).await;
    let me = endpoint.id();
    let room = Arc::new(Room {
        endpoint: endpoint.clone(),
        me,
        name: clean(&name),
        events: Mutex::new(VecDeque::new()),
        guests: Mutex::new(HashMap::new()),
        host: Mutex::new(None),
    });
    room.tell(Event::Room {
        ticket: encode_ticket(&endpoint.addr())?,
        me: me.to_string(),
    });
    let accepting = room.clone();
    tokio::spawn(async move {
        while let Some(incoming) = accepting.endpoint.accept().await {
            let room = accepting.clone();
            tokio::spawn(async move {
                let Ok(accepting) = incoming.accept() else {
                    return;
                };
                let Ok(connection) = accepting.await else {
                    return;
                };
                let _ = welcome(room, connection).await;
            });
        }
    });
    Ok(room)
}

/// Hosting: takes in a guest, tells it who is there and the others of it, and passes on what it says until it goes.
/// A guest coming back takes the place of the way it had before, which may not yet be known to be lost.
async fn welcome(room: Arc<Room>, connection: Connection) -> Result<()> {
    let id = connection.remote_id();
    let (mut send, mut recv) = connection.accept_bi().await?;
    let Wire::Hello { name } =
        tokio::time::timeout(Duration::from_secs(10), read(&mut recv)).await??
    else {
        bail!("no hello");
    };
    let name = clean(&name);
    {
        let guests = room.guests.lock().unwrap();
        if !guests.contains_key(&id) && guests.len() + 1 >= MAX_PLAYERS {
            connection.close(1u32.into(), b"full");
            bail!("full");
        }
    }
    write(
        &mut send,
        &Wire::Player {
            id: room.me.to_string(),
            name: room.name.clone(),
        },
    )
    .await?;
    let present: Vec<(EndpointId, String)> = room
        .guests
        .lock()
        .unwrap()
        .iter()
        .filter(|(other, _)| **other != id)
        .map(|(other, guest)| (*other, guest.name.clone()))
        .collect();
    for (other, other_name) in present {
        write(
            &mut send,
            &Wire::Player {
                id: other.to_string(),
                name: other_name,
            },
        )
        .await?;
    }
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_writer(send, rx);
    // The others hear of him even when he comes back, as he comes back into a fresh round.
    room.pass_on(
        &Wire::Player {
            id: id.to_string(),
            name: name.clone(),
        },
        Some(id),
    );
    let before = room.guests.lock().unwrap().insert(
        id,
        Guest {
            name: name.clone(),
            tx,
            connection: connection.clone(),
        },
    );
    if let Some(before) = before {
        before.connection.close(0u32.into(), b"replaced");
    }
    room.tell(Event::Player {
        id: id.to_string(),
        name,
    });
    // Throws go to everyone; who threw is who sent it, whatever it says.
    let mut last = tokio::time::Instant::now() - Duration::from_secs(1);
    while let Ok(wire) = read(&mut recv).await {
        if let Wire::Throw { at, .. } = wire {
            // A snowball every 150 ms at most from anyone: as fast as one can click, not as fast as a script.
            if last.elapsed() < Duration::from_millis(150) {
                continue;
            }
            last = tokio::time::Instant::now();
            let wire = Wire::Throw {
                from: id.to_string(),
                at: at.clone(),
            };
            room.pass_on(&wire, Some(id));
            room.tell(Event::Throw {
                from: id.to_string(),
                at,
            });
        }
    }
    {
        let mut guests = room.guests.lock().unwrap();
        if guests
            .get(&id)
            .is_none_or(|guest| guest.connection.stable_id() != connection.stable_id())
        {
            return Ok(());
        }
        guests.remove(&id);
    }
    room.pass_on(&Wire::Left { id: id.to_string() }, None);
    room.tell(Event::Left { id: id.to_string() });
    Ok(())
}

pub async fn join(ticket: String, name: String, relay: Option<String>) -> Result<Arc<Room>> {
    let addr = decode_ticket(&ticket)?;
    let name = clean(&name);
    let known = (addr.id, name.clone());
    let key = JOINED_AS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|keys| keys.get(&known).cloned());
    let endpoint = bind(relay, key).await?;
    JOINED_AS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(known, endpoint.secret_key().clone());
    let me = endpoint.id();
    let connection = tokio::time::timeout(Duration::from_secs(25), endpoint.connect(addr, ALPN))
        .await
        .map_err(|_| anyhow!("the room did not answer"))??;
    let (mut send, mut recv) = connection.open_bi().await?;
    write(&mut send, &Wire::Hello { name: name.clone() }).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_writer(send, rx);
    let room = Arc::new(Room {
        endpoint,
        me,
        name,
        events: Mutex::new(VecDeque::new()),
        guests: Mutex::new(HashMap::new()),
        host: Mutex::new(Some(tx)),
    });
    room.tell(Event::Joined { me: me.to_string() });
    let listening = room.clone();
    tokio::spawn(async move {
        loop {
            match read(&mut recv).await {
                Ok(Wire::Player { id, name }) if id != listening.me.to_string() => {
                    listening.tell(Event::Player {
                        id,
                        name: clean(&name),
                    })
                }
                Ok(Wire::Left { id }) => listening.tell(Event::Left { id }),
                Ok(Wire::Throw { from, at }) => listening.tell(Event::Throw { from, at }),
                Ok(_) => {}
                Err(_) => {
                    let reason = match connection.close_reason() {
                        Some(iroh::endpoint::ConnectionError::ApplicationClosed(close))
                            if close.reason.as_ref() == b"full" =>
                        {
                            "full"
                        }
                        _ => "gone",
                    };
                    listening.tell(Event::Closed {
                        reason: reason.to_string(),
                    });
                    break;
                }
            }
        }
    });
    Ok(room)
}

impl Room {
    /// Says that this player threw at `at` and hit.
    pub fn throw(&self, at: &str) {
        let wire = Wire::Throw {
            from: self.me.to_string(),
            at: at.to_string(),
        };
        if let Some(host) = self.host.lock().unwrap().as_ref() {
            let _ = host.send(wire);
        } else {
            self.pass_on(&wire, None);
        }
    }

    pub fn next(&self) -> Option<Event> {
        self.events.lock().unwrap().pop_front()
    }

    pub async fn leave(&self) {
        // Hosting, the guests learn the room is gone as their way to it closes.
        self.endpoint.close().await;
    }
}

fn string(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(text) }
        .to_str()
        .ok()
        .map(str::to_string)
}

fn give(text: String) -> *mut c_char {
    CString::new(text)
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

fn open(making: impl std::future::Future<Output = Result<Arc<Room>>> + Send + 'static) {
    leave_room();
    runtime().spawn(async move {
        match making.await {
            Ok(room) => *ROOM.lock().unwrap() = Some(room),
            Err(error) => tell_loose(Event::Error {
                message: format!("{error:#}"),
            }),
        }
    });
}

fn leave_room() {
    if let Some(room) = ROOM.lock().unwrap().take() {
        runtime().spawn(async move { room.leave().await });
    }
}

/// The room as the Windows and Linux apps, which link this crate, play in it: what the C functions do for the Mac.
pub mod play {
    use super::*;

    /// Opens a room as `name`, through `relay` if given; its invite comes as a `room` event.
    pub fn host(name: &str, relay: Option<&str>) {
        open(super::host(name.to_string(), relay.map(str::to_string)));
    }

    /// Joins the room of `ticket` as `name`; a `joined` event says it worked, an `error` event that it didn't.
    pub fn join(ticket: &str, name: &str, relay: Option<&str>) {
        open(super::join(
            ticket.to_string(),
            name.to_string(),
            relay.map(str::to_string),
        ));
    }

    /// Says this player hit the player `at` with a snowball.
    pub fn throw(at: &str) {
        if let Some(room) = ROOM.lock().unwrap().as_ref() {
            room.throw(at);
        }
    }

    /// The next thing that happened; none when there is none.
    pub fn next() -> Option<Event> {
        LOOSE
            .lock()
            .unwrap()
            .pop_front()
            .or_else(|| ROOM.lock().unwrap().as_ref().and_then(|room| room.next()))
    }

    pub fn leave() {
        leave_room();
    }

    /// Leaves the room and waits, half a second at most, for the others to be told: for when the app quits.
    pub fn leave_now() {
        if let Some(room) = ROOM.lock().unwrap().take() {
            runtime().block_on(async move {
                let _ = tokio::time::timeout(Duration::from_millis(500), room.leave()).await;
            });
        }
    }
}

/// Opens a room as `name`, through `relay` if given; its invite comes as a `room` event.
#[unsafe(no_mangle)]
pub extern "C" fn sn_host(name: *const c_char, relay: *const c_char) {
    play::host(&string(name).unwrap_or_default(), string(relay).as_deref());
}

/// Joins the room of `ticket` as `name`; a `joined` event says it worked, an `error` event that it didn't.
#[unsafe(no_mangle)]
pub extern "C" fn sn_join(ticket: *const c_char, name: *const c_char, relay: *const c_char) {
    play::join(
        &string(ticket).unwrap_or_default(),
        &string(name).unwrap_or_default(),
        string(relay).as_deref(),
    );
}

/// Says this player hit the player `at` with a snowball.
#[unsafe(no_mangle)]
pub extern "C" fn sn_throw(at: *const c_char) {
    if let Some(at) = string(at) {
        play::throw(&at);
    }
}

/// The next thing that happened, as JSON, to be given back to `sn_free`; null when there is none.
#[unsafe(no_mangle)]
pub extern "C" fn sn_next() -> *mut c_char {
    match play::next() {
        Some(event) => give(serde_json::to_string(&event).unwrap_or_default()),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sn_leave() {
    play::leave();
}

/// Leaves the room and waits, half a second at most, for the others to be told: for when the app quits.
#[unsafe(no_mangle)]
pub extern "C" fn sn_leave_now() {
    play::leave_now();
}

#[unsafe(no_mangle)]
pub extern "C" fn sn_free(text: *mut c_char) {
    if !text.is_null() {
        drop(unsafe { CString::from_raw(text) });
    }
}
