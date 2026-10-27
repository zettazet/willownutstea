mod avatars;
mod persistent_rooms;
use anyhow::{bail, ensure, Result};
use argon2::{
    password_hash::{PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
pub use avatars::optimize as optimize_avatar;
use mini_shared::*;
pub use persistent_rooms::{default_room_creator, default_room_password_hash};
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random<const N: usize>() -> [u8; N] {
    let mut b = [0; N];
    OsRng.fill_bytes(&mut b);
    b
}
fn hash(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
#[derive(Clone)]
pub struct Member {
    pub mic: bool,
    pub deaf: bool,
}
pub struct Room {
    pub id: u64,
    pub conversation: i64,
    pub members: HashMap<i64, Member>,
    pub invited: HashSet<i64>,
    pub toggles: HashSet<i64>,
}
pub enum Out {
    Json(Event),
    Audio(Vec<u8>),
    Close,
    Replaced,
}
pub struct Peer {
    pub tx: mpsc::Sender<Out>,
    pub generation: [u8; 16],
    pub expiry: u64,
    pub session: String,
    pub up: VoiceCipher,
    pub down: VoiceCipher,
    pub addr: Option<(std::net::SocketAddr, u64)>,
    pub voice_tick: u64,
    pub voice_count: u32,
    pub udp_enabled: bool,
    pub avatar_epoch: u64,
    pub room_attempt_after: u64,
    pub overloaded: std::sync::atomic::AtomicBool,
}
pub struct Server {
    pub db: Connection,
    pub peers: HashMap<i64, Peer>,
    pub rooms: HashMap<u64, Room>,
    pub udp_port: u16,
    rejoin: HashMap<i64, (u64, u64)>,
    uploads: HashMap<i64, avatars::Upload>,
    persistent: HashMap<u64, persistent_rooms::StoredRoom>,
    pub room_creator_username: String,
    pub room_creation_password_hash: String,
}
pub enum AuthWork {
    Register(String, String),
    Login(Option<(i64, String)>, String),
    Resume(String),
}
pub enum VerifiedAuth {
    Register(String, String),
    Login(i64),
    Resume(String),
}
impl AuthWork {
    pub fn verify(self) -> Result<VerifiedAuth> {
        match self {
            Self::Register(username, password) => {
                let salt = SaltString::generate(&mut OsRng);
                let hash = Argon2::default()
                    .hash_password(password.as_bytes(), &salt)
                    .map_err(|_| anyhow::anyhow!("Hashing failed"))?
                    .to_string();
                Ok(VerifiedAuth::Register(username, hash))
            }
            Self::Login(row, password) => {
                let dummy="$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$5LP0ZnGGwG+Z5xQ8euy5gIsFYr9Cqy4gbcSnmV5idPQ";
                let p = PasswordHash::new(row.as_ref().map(|x| x.1.as_str()).unwrap_or(dummy))
                    .map_err(|_| anyhow::anyhow!("Invalid credentials"))?;
                let ok = Argon2::default()
                    .verify_password(password.as_bytes(), &p)
                    .is_ok();
                ensure!(ok && row.is_some(), "Invalid credentials");
                Ok(VerifiedAuth::Login(row.unwrap().0))
            }
            Self::Resume(token) => Ok(VerifiedAuth::Resume(token)),
        }
    }
}
impl Server {
    pub fn open(path: &str, udp_port: u16) -> Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=3000;
 CREATE TABLE IF NOT EXISTS users(id INTEGER PRIMARY KEY, username TEXT NOT NULL COLLATE NOCASE UNIQUE, password TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS sessions(token TEXT PRIMARY KEY,user INTEGER NOT NULL REFERENCES users(id),expires INTEGER NOT NULL);
 CREATE TABLE IF NOT EXISTS friends(a INTEGER NOT NULL REFERENCES users(id),b INTEGER NOT NULL REFERENCES users(id),accepted INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(a,b),CHECK(a<>b));
 CREATE TABLE IF NOT EXISTS conversations(id INTEGER PRIMARY KEY);
 CREATE TABLE IF NOT EXISTS members(conversation INTEGER REFERENCES conversations(id),user INTEGER REFERENCES users(id),PRIMARY KEY(conversation,user));
 CREATE TABLE IF NOT EXISTS messages(id INTEGER PRIMARY KEY,conversation INTEGER NOT NULL REFERENCES conversations(id),sender INTEGER NOT NULL REFERENCES users(id),timestamp INTEGER NOT NULL,nonce TEXT NOT NULL,text TEXT NOT NULL,UNIQUE(sender,nonce));
 CREATE INDEX IF NOT EXISTS message_page ON messages(conversation,id);
 CREATE TABLE IF NOT EXISTS avatars(user INTEGER PRIMARY KEY REFERENCES users(id), revision TEXT NOT NULL, data BLOB NOT NULL);
 CREATE INDEX IF NOT EXISTS user_members ON members(user,conversation);")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS voice_rooms(id INTEGER PRIMARY KEY,owner INTEGER NOT NULL REFERENCES users(id),name TEXT NOT NULL COLLATE NOCASE,UNIQUE(owner,name));")?;
        let mut server = Self {
            db,
            peers: HashMap::new(),
            rooms: HashMap::new(),
            udp_port,
            rejoin: HashMap::new(),
            uploads: HashMap::new(),
            persistent: HashMap::new(),
            room_creator_username: default_room_creator(),
            room_creation_password_hash: default_room_password_hash(),
        };
        server.load_voice_rooms()?;
        Ok(server)
    }
    pub fn user(&self, id: i64) -> Result<User> {
        Ok(self
            .db
            .query_row("SELECT id,username,(SELECT revision FROM avatars WHERE user=users.id) FROM users WHERE id=?", [id], |r| {
                Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    avatar: r.get(2)?,
                })
            })?)
    }
    /// Read account metadata briefly under the server lock, then perform Argon2 off-lock.
    pub fn auth_work(&self, r: Request) -> Result<AuthWork> {
        match r {
            Request::Register { username, password } => {
                ensure!(
                    valid_username(&username),
                    "Username: 3–24 ASCII letters, digits or underscore"
                );
                ensure!(
                    (10..=128).contains(&password.len()),
                    "Password: 10–128 bytes"
                );
                let exists: bool = self.db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE username=?)",
                    [&username],
                    |r| r.get(0),
                )?;
                ensure!(!exists, "Username unavailable");
                Ok(AuthWork::Register(username, password))
            }
            Request::Login { username, password } => {
                ensure!(
                    valid_username(&username) && password.len() <= 128,
                    "Invalid credentials"
                );
                let row = self
                    .db
                    .query_row(
                        "SELECT id,password FROM users WHERE username=?",
                        [username],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                Ok(AuthWork::Login(row, password))
            }
            Request::Resume { token } => {
                ensure!(token.len() == 64, "Invalid session");
                Ok(AuthWork::Resume(token))
            }
            _ => bail!("Authenticate first"),
        }
    }
    pub fn auth(&mut self, r: Request, tx: mpsc::Sender<Out>) -> Result<(i64, [u8; 16])> {
        let proof = self.auth_work(r)?.verify()?;
        self.auth_verified(proof, tx)
    }
    pub fn auth_verified(
        &mut self,
        proof: VerifiedAuth,
        tx: mpsc::Sender<Out>,
    ) -> Result<(i64, [u8; 16])> {
        self.db
            .execute("DELETE FROM sessions WHERE expires<=?", [now()])?;
        let (id, token, expiry) = match proof {
            VerifiedAuth::Resume(token) => {
                let (id, e) = self
                    .db
                    .query_row(
                        "SELECT user,expires FROM sessions WHERE token=? AND expires>?",
                        params![hash(&token), now()],
                        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u64>(1)?)),
                    )
                    .map_err(|_| anyhow::anyhow!("Session expired"))?;
                (id, token, e)
            }
            proof => {
                let id = match proof {
                    VerifiedAuth::Register(username, p) => {
                        self.db
                            .execute(
                                "INSERT INTO users(username,password) VALUES(?,?)",
                                params![username, p],
                            )
                            .map_err(|_| anyhow::anyhow!("Username unavailable"))?;
                        self.db.last_insert_rowid()
                    }
                    VerifiedAuth::Login(id) => id,
                    _ => unreachable!(),
                };
                let token = hex::encode(random::<32>());
                let expiry = now() + 30 * 86400;
                self.db.execute(
                    "INSERT INTO sessions VALUES(?,?,?)",
                    params![hash(&token), id, expiry],
                )?;
                self.db.execute("DELETE FROM sessions WHERE user=? AND token NOT IN (SELECT token FROM sessions WHERE user=? ORDER BY expires DESC,rowid DESC LIMIT 8)",params![id,id])?;
                (id, token, expiry)
            }
        };
        self.uploads.remove(&id);
        let generation = random::<16>();
        let key = random::<32>();
        let user = self.user(id)?;
        self.expire_rejoins();
        if let Some(old) = self.peers.remove(&id) {
            self.leave(id);
            let _ = old.tx.try_send(Out::Replaced);
        }
        self.peers.insert(
            id,
            Peer {
                tx: tx.clone(),
                generation,
                expiry,
                session: hash(&token),
                up: VoiceCipher::new(&key, generation, b"up"),
                down: VoiceCipher::new(&key, generation, b"down"),
                addr: None,
                voice_tick: now(),
                voice_count: 0,
                udp_enabled: false,
                avatar_epoch: 0,
                room_attempt_after: 0,
                overloaded: std::sync::atomic::AtomicBool::new(false),
            },
        );
        let _ = tx.try_send(Out::Json(Event::Auth {
            user,
            token,
            voice_id: generation,
            voice_key: key,
            udp_port: self.udp_port,
        }));
        self.refresh();
        Ok((id, generation))
    }
    pub fn emit(&self, id: i64, e: Event) {
        if let Some(p) = self.peers.get(&id) {
            if p.tx.try_send(Out::Json(e)).is_err() {
                // A full queue forces reconnect rather than unbounded memory growth.
                p.overloaded
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    pub fn friend(&self, a: i64, b: i64) -> bool {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM friends WHERE accepted=1 AND ((a=? AND b=?) OR (a=? AND b=?)))",params![a,b,b,a],|r|r.get(0)).unwrap_or(false)
    }
    fn users_query(&self, sql: &str, id: i64) -> Result<Vec<User>> {
        let mut s = self.db.prepare(sql)?;
        let rows = s.query_map([id], |r| {
            Ok(User {
                id: r.get(0)?,
                username: r.get(1)?,
                avatar: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn authorized(&self, u: i64, c: i64) -> bool {
        self.db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM members WHERE user=? AND conversation=?)",
                params![u, c],
                |r| r.get(0),
            )
            .unwrap_or(false)
    }
    pub fn snapshot(&self, id: i64) -> Result<Snapshot> {
        let friends=self.users_query("SELECT id,username,(SELECT revision FROM avatars WHERE user=users.id) FROM users WHERE id IN (SELECT CASE WHEN a=?1 THEN b ELSE a END FROM friends WHERE accepted=1 AND (a=?1 OR b=?1)) ORDER BY username",id)?;
        let incoming=self.users_query("SELECT id,username,(SELECT revision FROM avatars WHERE user=users.id) FROM users WHERE id IN (SELECT a FROM friends WHERE b=?1 AND accepted=0)",id)?;
        let mut conversations = Vec::new();
        let mut s = self.db.prepare(
            "SELECT conversation FROM members WHERE user=? ORDER BY conversation DESC LIMIT 128",
        )?;
        for c in s.query_map([id], |r| r.get::<_, i64>(0))? {
            let c = c?;
            conversations.push(Conversation{id:c,users:self.users_query("SELECT id,username,(SELECT revision FROM avatars WHERE user=users.id) FROM users WHERE id IN (SELECT user FROM members WHERE conversation=?) ORDER BY id",c)?});
        }
        let mut calls = Vec::new();
        for room in self
            .rooms
            .values()
            .filter(|r| r.members.contains_key(&id) || r.invited.contains(&id))
        {
            let mut participants = Vec::new();
            for (&u, m) in &room.members {
                participants.push(Participant {
                    user: self.user(u)?,
                    mic_muted: m.mic,
                    deafened: m.deaf,
                    labeled: room.toggles.iter().any(|&actor| actor != u),
                });
            }
            participants.sort_by_key(|p| p.user.id);
            calls.push(Call {
                id: room.id,
                conversation: room.conversation,
                participants,
                invited: room.invited.iter().copied().collect(),
            });
        }
        calls.sort_by_key(|c| c.id);
        Ok(Snapshot {
            friends,
            incoming,
            conversations,
            calls,
            voice_rooms: self.visible_voice_rooms(id)?,
            can_create_rooms: self
                .user(id)?
                .username
                .eq_ignore_ascii_case(&self.room_creator_username),
        })
    }
    pub fn refresh(&self) {
        for &id in self.peers.keys() {
            if let Ok(state) = self.snapshot(id) {
                self.emit(id, Event::State { state });
            }
        }
    }
    pub fn expire_rejoins(&mut self) {
        self.expire_avatar_uploads();
        let expired: Vec<_> = self
            .rejoin
            .iter()
            .filter(|(_, (_, t))| *t <= now())
            .map(|(&u, _)| u)
            .collect();
        for u in expired {
            self.rejoin.remove(&u);
            for r in self.rooms.values_mut() {
                r.invited.remove(&u);
            }
        }
    }
    pub fn leave(&mut self, id: i64) {
        self.rejoin.remove(&id);
        let persistent = &self.persistent;
        self.rooms.retain(|room_id, r| {
            r.members.remove(&id);
            r.invited.remove(&id);
            r.toggles.remove(&id);
            !r.members.is_empty() || persistent.contains_key(room_id)
        });
    }
    pub fn disconnect(&mut self, id: i64, g: [u8; 16]) {
        if self.peers.get(&id).is_some_and(|p| p.generation == g) {
            self.uploads.remove(&id);
        }
        if self.peers.get(&id).is_some_and(|p| p.generation == g) {
            self.peers.remove(&id);
            for r in self.rooms.values_mut() {
                if r.members.remove(&id).is_some() {
                    r.toggles.remove(&id);
                    if !r.members.is_empty() {
                        r.invited.insert(id);
                        self.rejoin.insert(id, (r.id, now() + 30));
                    }
                }
            }
            self.rooms
                .retain(|id, r| !r.members.is_empty() || self.persistent.contains_key(id));
            self.refresh();
        }
    }
    pub fn handle(&mut self, u: i64, req: Request) -> Result<()> {
        ensure!(
            self.peers.get(&u).is_some_and(|p| p.expiry > now()),
            "Session expired"
        );
        match req {
            Request::AvatarChunk {
                total,
                offset,
                data,
            } => {
                self.avatar_chunk(u, total, offset, data)?;
                return Ok(());
            }
            Request::AvatarGet { user } => {
                self.avatar_get(u, user)?;
                return Ok(());
            }
            Request::AvatarRemove {} => {
                if let Some(peer) = self.peers.get_mut(&u) {
                    peer.avatar_epoch = peer.avatar_epoch.wrapping_add(1);
                }
                self.uploads.remove(&u);
                self.db.execute("DELETE FROM avatars WHERE user=?", [u])?;
                self.emit(u, Event::AvatarSaved { revision: None });
            }
            Request::Logout {} => {
                self.uploads.remove(&u);
                let token = self.peers[&u].session.clone();
                self.db
                    .execute("DELETE FROM sessions WHERE token=?", [token])?;
                self.emit(u, Event::LoggedOut);
                self.leave(u);
                self.peers.remove(&u);
            }
            Request::VoiceTransport { udp } => {
                self.peers.get_mut(&u).unwrap().udp_enabled = udp;
                self.emit(u, Event::Pong);
                return Ok(());
            }
            Request::Sync {} => {}
            Request::FriendRequest { username } => {
                ensure!(valid_username(&username), "Invalid username");
                let b = self
                    .db
                    .query_row("SELECT id FROM users WHERE username=?", [username], |r| {
                        r.get::<_, i64>(0)
                    })
                    .map_err(|_| anyhow::anyhow!("User not found"))?;
                ensure!(u != b, "Cannot add yourself");
                ensure!(!self.friend(u, b), "Already friends");
                let count: i64 = self.db.query_row(
                    "SELECT COUNT(*) FROM friends WHERE a=? OR b=?",
                    params![u, u],
                    |r| r.get(0),
                )?;
                ensure!(count < 128, "Friend limit reached");
                let count: i64 = self.db.query_row(
                    "SELECT COUNT(*) FROM friends WHERE a=? OR b=?",
                    params![b, b],
                    |r| r.get(0),
                )?;
                ensure!(count < 128, "Friend's list is full");
                self.db.execute(
                    "INSERT OR IGNORE INTO friends(a,b) VALUES(?,?)",
                    params![u, b],
                )?;
            }
            Request::FriendAccept { user } => {
                let t = self.db.transaction()?;
                ensure!(
                    t.execute(
                        "UPDATE friends SET accepted=1 WHERE a=? AND b=? AND accepted=0",
                        params![user, u]
                    )? > 0,
                    "No incoming request"
                );
                // Two people requesting each other express mutual consent. Keep a
                // single accepted relationship, not a second stale incoming request.
                t.execute(
                    "DELETE FROM friends WHERE a=? AND b=? AND accepted=0",
                    params![u, user],
                )?;
                t.commit()?;
            }
            Request::Conversation { mut users } => {
                users.sort_unstable();
                users.dedup();
                users.retain(|x| *x != u);
                ensure!(
                    !users.is_empty() && users.len() < MAX_CALL,
                    "Choose 1–7 friends"
                );
                for &b in &users {
                    ensure!(self.friend(u, b), "Conversation members must be friends");
                }
                users.push(u);
                users.sort_unstable();
                let count: i64 =
                    self.db
                        .query_row("SELECT COUNT(*) FROM members WHERE user=?", [u], |r| {
                            r.get(0)
                        })?;
                ensure!(count < 128, "Conversation limit reached");
                let existing = self.snapshot(u)?.conversations.into_iter().any(|c| {
                    let mut ids: Vec<_> = c.users.iter().map(|x| x.id).collect();
                    ids.sort_unstable();
                    ids == users
                });
                if !existing {
                    let t = self.db.transaction()?;
                    t.execute("INSERT INTO conversations DEFAULT VALUES", [])?;
                    let c = t.last_insert_rowid();
                    for id in users {
                        t.execute("INSERT INTO members VALUES(?,?)", params![c, id])?;
                    }
                    t.commit()?;
                }
            }
            Request::History {
                conversation,
                before,
            } => {
                ensure!(
                    self.authorized(u, conversation),
                    "Conversation access denied"
                );
                let mut s=self.db.prepare("SELECT m.id,m.conversation,u.id,u.username,m.timestamp,m.nonce,m.text FROM messages m JOIN users u ON u.id=m.sender WHERE conversation=? AND m.id<? ORDER BY m.id DESC LIMIT 50")?;
                let mut messages = s
                    .query_map(params![conversation, before.unwrap_or(i64::MAX)], |r| {
                        Ok(ChatMessage {
                            id: r.get(0)?,
                            conversation: r.get(1)?,
                            sender: User {
                                id: r.get(2)?,
                                username: r.get(3)?,
                                avatar: None,
                            },
                            timestamp: r.get(4)?,
                            nonce: r.get(5)?,
                            text: r.get(6)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                messages.reverse();
                self.emit(
                    u,
                    Event::History {
                        conversation,
                        messages,
                    },
                );
                return Ok(());
            }
            Request::Send {
                conversation,
                nonce,
                text,
            } => {
                ensure!(
                    self.authorized(u, conversation),
                    "Conversation access denied"
                );
                ensure!(
                    !text.trim().is_empty()
                        && !text
                            .chars()
                            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
                        && text.len() <= MAX_TEXT
                        && nonce.len() == 32
                        && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Invalid message"
                );
                let old: Option<(i64, i64, String)> = self
                    .db
                    .query_row(
                        "SELECT id,conversation,text FROM messages WHERE sender=? AND nonce=?",
                        params![u, nonce],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                if let Some((id, c, t)) = old {
                    ensure!(
                        c == conversation && t == text,
                        "Nonce reused for different content"
                    );
                    self.emit(u, Event::Ack { nonce, id });
                    return Ok(());
                }
                let timestamp = now();
                self.db.execute("INSERT INTO messages(conversation,sender,timestamp,nonce,text) VALUES(?,?,?,?,?)",params![conversation,u,timestamp,nonce,text])?;
                let id = self.db.last_insert_rowid();
                let message = ChatMessage {
                    id,
                    conversation,
                    sender: self.user(u)?,
                    timestamp,
                    nonce: nonce.clone(),
                    text,
                };
                let mut s = self
                    .db
                    .prepare("SELECT user FROM members WHERE conversation=?")?;
                for member in s.query_map([conversation], |r| r.get::<_, i64>(0))? {
                    self.emit(
                        member?,
                        Event::Message {
                            message: message.clone(),
                        },
                    );
                }
                self.emit(u, Event::Ack { nonce, id });
                return Ok(());
            }
            Request::RoomCreate { name, password } => self.create_voice_room(u, name, password)?,
            Request::RoomDelete { room } => self.delete_voice_room(u, room)?,
            Request::Call { conversation } => {
                ensure!(
                    self.authorized(u, conversation),
                    "Conversation access denied"
                );
                ensure!(
                    !self.rooms.values().any(|r| r.members.contains_key(&u)),
                    "Already in a call"
                );
                ensure!(
                    !self.rooms.values().any(|r| r.conversation == conversation),
                    "Call already exists; accept its invitation"
                );
                ensure!(self.rooms.len() < 128, "Server call limit reached");
                let id = self.unused_call_id();
                let users=self.users_query("SELECT id,username,(SELECT revision FROM avatars WHERE user=users.id) FROM users WHERE id IN (SELECT user FROM members WHERE conversation=?)",conversation)?;
                self.rooms.insert(
                    id,
                    Room {
                        id,
                        conversation,
                        members: HashMap::from([(
                            u,
                            Member {
                                mic: false,
                                deaf: false,
                            },
                        )]),
                        invited: users
                            .into_iter()
                            .map(|x| x.id)
                            .filter(|x| *x != u)
                            .collect(),
                        toggles: HashSet::new(),
                    },
                );
            }
            Request::Invite { call, user } => {
                ensure!(
                    !self.persistent.contains_key(&call),
                    "Friends of the room owner join rooms directly"
                );
                ensure!(
                    self.rooms
                        .get(&call)
                        .is_some_and(|r| r.members.contains_key(&u)),
                    "Call access denied"
                );
                ensure!(self.friend(u, user), "Invite friends only");
                let r = self.rooms.get(&call).unwrap();
                ensure!(
                    !r.members.contains_key(&user) && r.members.len() + r.invited.len() < MAX_CALL,
                    "Call is full or user already invited"
                );
                self.rooms.get_mut(&call).unwrap().invited.insert(user);
            }
            Request::Join { call } => {
                ensure!(
                    !self.rooms.values().any(|r| r.members.contains_key(&u)),
                    "Already in a call"
                );
                let persistent = self.persistent.contains_key(&call);
                if persistent {
                    ensure!(self.can_join_voice_room(u, call), "Room access denied");
                }
                let r = self
                    .rooms
                    .get_mut(&call)
                    .ok_or_else(|| anyhow::anyhow!("Call ended"))?;
                ensure!(persistent || r.invited.contains(&u), "Not invited");
                ensure!(r.members.len() < MAX_CALL, "Call full");
                r.invited.remove(&u);
                self.rejoin.remove(&u);
                r.members.insert(
                    u,
                    Member {
                        mic: false,
                        deaf: false,
                    },
                );
            }
            Request::Decline { call } => {
                self.rejoin.remove(&u);
                if let Some(r) = self.rooms.get_mut(&call) {
                    r.invited.remove(&u);
                }
            }
            Request::Leave {} => self.leave(u),
            Request::Controls {
                mic_muted,
                deafened,
            } => {
                let r = self
                    .rooms
                    .values_mut()
                    .find(|r| r.members.contains_key(&u))
                    .ok_or_else(|| anyhow::anyhow!("Not in a call"))?;
                r.members.insert(
                    u,
                    Member {
                        mic: mic_muted,
                        deaf: deafened,
                    },
                );
            }
            Request::Toggle {} => {
                let r = self
                    .rooms
                    .values_mut()
                    .find(|r| r.members.contains_key(&u))
                    .ok_or_else(|| anyhow::anyhow!("Not in a call"))?;
                if !r.toggles.remove(&u) {
                    r.toggles.insert(u);
                }
            }
            _ => bail!("Already authenticated"),
        }
        self.refresh();
        Ok(())
    }
    /// Authorization and mute/deafen are checked on every packet, for both transports.
    pub fn relay(&mut self, u: i64, audio: &[u8]) -> Vec<(std::net::SocketAddr, Vec<u8>)> {
        if audio.len() <= 16 || audio.len() > MAX_OPUS + 16 {
            return vec![];
        }
        let Some(p) = self.peers.get_mut(&u) else {
            return vec![];
        };
        if p.expiry <= now() {
            return vec![];
        }
        if p.voice_tick != now() {
            p.voice_tick = now();
            p.voice_count = 0;
        }
        p.voice_count += 1;
        if p.voice_count > 60 {
            return vec![];
        }
        let call_id = u64::from_be_bytes(audio[..8].try_into().unwrap());
        let Some(room) = self
            .rooms
            .values()
            .find(|r| r.id == call_id && r.members.get(&u).is_some_and(|m| !m.mic))
        else {
            return vec![];
        };
        let recipients: Vec<_> = room
            .members
            .iter()
            .filter(|(id, m)| **id != u && !m.deaf)
            .map(|(&id, _)| id)
            .collect();
        let mut data = call_id.to_be_bytes().to_vec();
        data.extend_from_slice(&u.to_be_bytes());
        data.extend_from_slice(&audio[8..]);
        let mut udp = vec![];
        for id in recipients {
            if let Some(p) = self.peers.get_mut(&id) {
                if let Some((addr, t)) = p
                    .addr
                    .filter(|(_, t)| p.udp_enabled && now().saturating_sub(*t) < 3)
                {
                    let _ = t;
                    if let Some(packet) = p.down.seal(&data) {
                        udp.push((addr, packet));
                    }
                } else {
                    let _ = p.tx.try_send(Out::Audio(data.clone()));
                }
            }
        }
        udp
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn reg(s: &mut Server, n: &str) -> i64 {
        let (tx, _) = mpsc::channel(128);
        s.auth(
            Request::Register {
                username: n.into(),
                password: "long password 123".into(),
            },
            tx,
        )
        .unwrap()
        .0
    }
    #[test]
    fn accounts_friends_messages_calls() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut s = Server::open(path.to_str().unwrap(), 4444).unwrap();
        let a = reg(&mut s, "Alex");
        let b = reg(&mut s, "Ben");
        let c = reg(&mut s, "Chris");
        let (tx, _) = mpsc::channel(128);
        assert!(s
            .auth(
                Request::Login {
                    username: "Alex".into(),
                    password: "wrong".into()
                },
                tx
            )
            .is_err());
        assert!(s
            .handle(a, Request::Conversation { users: vec![b] })
            .is_err());
        s.handle(
            a,
            Request::FriendRequest {
                username: "Ben".into(),
            },
        )
        .unwrap();
        s.handle(b, Request::FriendAccept { user: a }).unwrap();
        s.handle(a, Request::Conversation { users: vec![b] })
            .unwrap();
        let convo = s.snapshot(a).unwrap().conversations[0].id;
        let req = Request::Send {
            conversation: convo,
            nonce: "0".repeat(32),
            text: "hello".into(),
        };
        s.handle(a, req.clone()).unwrap();
        s.handle(a, req).unwrap();
        assert_eq!(
            s.db.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(s
            .handle(
                c,
                Request::History {
                    conversation: convo,
                    before: None
                }
            )
            .is_err());
        s.handle(
            a,
            Request::Call {
                conversation: convo,
            },
        )
        .unwrap();
        let call = *s.rooms.keys().next().unwrap();
        assert!(s.handle(c, Request::Join { call }).is_err());
        s.handle(a, Request::Toggle {}).unwrap();
        s.handle(b, Request::Join { call }).unwrap();
        let room = &s.snapshot(a).unwrap().calls[0];
        assert!(!room.participants[0].labeled);
        assert!(room.participants[1].labeled);
        s.handle(b, Request::Toggle {}).unwrap();
        assert!(s.snapshot(a).unwrap().calls[0]
            .participants
            .iter()
            .all(|x| x.labeled));
        s.handle(a, Request::Toggle {}).unwrap();
        let r = &s.snapshot(a).unwrap().calls[0];
        assert!(r.participants[0].labeled && !r.participants[1].labeled);
        s.handle(
            b,
            Request::Controls {
                mic_muted: true,
                deafened: false,
            },
        )
        .unwrap();
        assert!(s.relay(b, &[1]).is_empty());
        s.handle(
            a,
            Request::Controls {
                mic_muted: false,
                deafened: true,
            },
        )
        .unwrap();
        assert!(s.relay(b, &[1]).is_empty());
        s.handle(b, Request::Leave {}).unwrap();
        assert!(!s.snapshot(a).unwrap().calls[0].participants[0].labeled);
        s.handle(a, Request::Leave {}).unwrap();
        assert!(s.rooms.is_empty());
        drop(s);
        let s = Server::open(path.to_str().unwrap(), 4444).unwrap();
        assert_eq!(s.snapshot(a).unwrap().conversations.len(), 1);
        assert!(s.friend(a, b));
    }
    #[test]
    fn sessions_and_disconnect_generation() {
        let mut s = Server::open(":memory:", 0).unwrap();
        let (tx, mut rx) = mpsc::channel(32);
        let (a, g) = s
            .auth(
                Request::Register {
                    username: "Alex".into(),
                    password: "long password".into(),
                },
                tx,
            )
            .unwrap();
        let token = match rx.try_recv().unwrap() {
            Out::Json(Event::Auth { token, .. }) => token,
            _ => panic!(),
        };
        let (tx, _) = mpsc::channel(32);
        let (_, new) = s
            .auth(
                Request::Resume {
                    token: token.clone(),
                },
                tx,
            )
            .unwrap();
        s.disconnect(a, g);
        assert_eq!(s.peers[&a].generation, new);
        s.handle(a, Request::Logout {}).unwrap();
        let (tx, _) = mpsc::channel(32);
        assert!(s.auth(Request::Resume { token }, tx).is_err());
    }
    #[test]
    fn crossed_friend_requests_leave_no_stale_invitation() {
        let mut s = Server::open(":memory:", 0).unwrap();
        let a = reg(&mut s, "Alex");
        let b = reg(&mut s, "Ben");
        s.handle(
            a,
            Request::FriendRequest {
                username: "Ben".into(),
            },
        )
        .unwrap();
        s.handle(
            b,
            Request::FriendRequest {
                username: "Alex".into(),
            },
        )
        .unwrap();
        s.handle(a, Request::FriendAccept { user: b }).unwrap();
        assert!(s.friend(a, b));
        assert!(s.snapshot(a).unwrap().incoming.is_empty());
        assert!(s.snapshot(b).unwrap().incoming.is_empty());
        assert_eq!(s.snapshot(a).unwrap().friends.len(), 1);
        assert!(s.handle(b, Request::FriendAccept { user: a }).is_err());
        assert!(s.friend(a, b));
    }
}
