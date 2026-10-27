use super::*;
pub fn default_room_creator() -> String {
    "judas".into()
}
pub fn default_room_password_hash() -> String {
    // Creation PIN is checked only on the server, after account authorization.
    hash("231939")
}
pub struct StoredRoom {
    pub owner: i64,
    pub name: String,
}
impl Server {
    pub(super) fn load_voice_rooms(&mut self) -> Result<()> {
        let mut statement = self
            .db
            .prepare("SELECT id,owner,name FROM voice_rooms ORDER BY id")?;
        let rows = statement.query_map([], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, owner, name) = row?;
            ensure!(
                self.persistent.len() < 32,
                "Persistent room limit exceeded in database"
            );
            self.persistent.insert(id, StoredRoom { owner, name });
            self.rooms.insert(
                id,
                Room {
                    id,
                    conversation: 0,
                    members: HashMap::new(),
                    invited: HashSet::new(),
                    toggles: HashSet::new(),
                },
            );
        }
        Ok(())
    }
    pub(super) fn unused_call_id(&self) -> u64 {
        loop {
            let id = u64::from_be_bytes(random::<8>()) >> 1;
            if id != 0 && !self.rooms.contains_key(&id) {
                return id;
            }
        }
    }
    pub(super) fn can_join_voice_room(&self, user: i64, room: u64) -> bool {
        self.persistent
            .get(&room)
            .is_some_and(|r| r.owner == user || self.friend(r.owner, user))
    }
    pub(super) fn visible_voice_rooms(&self, user: i64) -> Result<Vec<VoiceRoom>> {
        let mut visible = Vec::new();
        for (&id, room) in &self.persistent {
            if self.can_join_voice_room(user, id) {
                visible.push(VoiceRoom {
                    id,
                    name: room.name.clone(),
                    owner: self.user(room.owner)?,
                    participants: self.rooms.get(&id).map_or(0, |r| r.members.len()),
                });
            }
        }
        visible.sort_by_key(|r| r.id);
        Ok(visible)
    }
    pub(super) fn create_voice_room(
        &mut self,
        user: i64,
        name: String,
        password: String,
    ) -> Result<()> {
        ensure!(
            self.user(user)?
                .username
                .eq_ignore_ascii_case(&self.room_creator_username),
            "Only the configured room creator may create rooms"
        );
        let peer = self
            .peers
            .get_mut(&user)
            .ok_or_else(|| anyhow::anyhow!("Session expired"))?;
        ensure!(
            now() >= peer.room_attempt_after,
            "Wait 3 seconds before creating another room"
        );
        peer.room_attempt_after = now() + 3;
        ensure!(
            password.len() <= 128 && hash(&password) == self.room_creation_password_hash,
            "Incorrect room creation password"
        );
        let name = name.trim();
        ensure!(
            (1..=48).contains(&name.chars().count()) && !name.chars().any(char::is_control),
            "Room name must contain 1–48 visible characters"
        );
        ensure!(
            self.persistent.len() < 32 && self.rooms.len() < 128,
            "Server room limit reached"
        );
        let id = self.unused_call_id();
        self.db
            .execute(
                "INSERT INTO voice_rooms(id,owner,name) VALUES(?,?,?)",
                params![id, user, name],
            )
            .map_err(|_| anyhow::anyhow!("Cannot create room; name may already exist"))?;
        self.persistent.insert(
            id,
            StoredRoom {
                owner: user,
                name: name.into(),
            },
        );
        self.rooms.insert(
            id,
            Room {
                id,
                conversation: 0,
                members: HashMap::new(),
                invited: HashSet::new(),
                toggles: HashSet::new(),
            },
        );
        Ok(())
    }
    pub(super) fn delete_voice_room(&mut self, user: i64, id: u64) -> Result<()> {
        ensure!(
            self.persistent.get(&id).is_some_and(|r| r.owner == user),
            "Only the room owner may delete it"
        );
        self.db
            .execute("DELETE FROM voice_rooms WHERE id=?", [id])?;
        self.rooms.remove(&id);
        self.persistent.remove(&id);
        self.rejoin.retain(|_, (room, _)| *room != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account(server: &mut Server, name: &str) -> (i64, [u8; 16]) {
        let (tx, _) = mpsc::channel(128);
        server
            .auth(
                Request::Register {
                    username: name.into(),
                    password: "account password 123".into(),
                },
                tx,
            )
            .unwrap()
    }
    fn create(server: &mut Server, user: i64, name: &str, pin: &str) -> Result<()> {
        server.peers.get_mut(&user).unwrap().room_attempt_after = 0;
        server.handle(
            user,
            Request::RoomCreate {
                name: name.into(),
                password: pin.into(),
            },
        )
    }
    #[test]
    fn rooms_require_owner_and_pin_persist_empty_and_enforce_friend_access() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rooms.sqlite");
        let path = path.to_str().unwrap();
        let mut server = Server::open(path, 4434).unwrap();
        let (owner, owner_generation) = account(&mut server, "judas");
        let (friend, friend_generation) = account(&mut server, "friend");
        let (stranger, _) = account(&mut server, "stranger");
        assert!(create(&mut server, stranger, "Unauthorized", "231939").is_err());
        assert!(create(&mut server, owner, "Wrong PIN", "000000").is_err());
        assert!(create(&mut server, owner, "bad\nname", "231939").is_err());
        create(&mut server, owner, "Warm lounge", "231939").unwrap();
        assert!(server.snapshot(owner).unwrap().can_create_rooms);
        assert!(!server.snapshot(friend).unwrap().can_create_rooms);
        let error = server
            .handle(
                owner,
                Request::RoomCreate {
                    name: "Too soon".into(),
                    password: "231939".into(),
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("Wait 3 seconds"));
        assert!(create(&mut server, owner, "warm lounge", "231939").is_err());
        let room = server.snapshot(owner).unwrap().voice_rooms[0].id;
        assert_eq!(
            server.snapshot(owner).unwrap().voice_rooms[0].participants,
            0
        );
        assert!(server.snapshot(friend).unwrap().voice_rooms.is_empty());
        assert!(server.handle(friend, Request::Join { call: room }).is_err());
        server
            .handle(
                owner,
                Request::FriendRequest {
                    username: "friend".into(),
                },
            )
            .unwrap();
        server
            .handle(friend, Request::FriendAccept { user: owner })
            .unwrap();
        assert_eq!(server.snapshot(friend).unwrap().voice_rooms.len(), 1);
        server.disconnect(owner, owner_generation);
        server.handle(friend, Request::Join { call: room }).unwrap();
        assert_eq!(
            server.snapshot(friend).unwrap().voice_rooms[0].participants,
            1
        );
        assert!(server
            .handle(stranger, Request::Join { call: room })
            .is_err());
        assert!(server
            .handle(
                friend,
                Request::Invite {
                    call: room,
                    user: stranger
                }
            )
            .is_err());
        assert!(server.handle(friend, Request::RoomDelete { room }).is_err());
        server.disconnect(friend, friend_generation);
        assert!(server.rooms.contains_key(&room));
        drop(server);
        let mut server = Server::open(path, 4434).unwrap();
        assert_eq!(server.snapshot(owner).unwrap().voice_rooms[0].id, room);
        assert_eq!(
            server.snapshot(friend).unwrap().voice_rooms[0].participants,
            0
        );
        // Reauthenticate existing accounts; restart never restores microphones.
        let (tx, _) = mpsc::channel(128);
        server
            .auth(
                Request::Login {
                    username: "judas".into(),
                    password: "account password 123".into(),
                },
                tx,
            )
            .unwrap();
        let (tx, _) = mpsc::channel(128);
        server
            .auth(
                Request::Login {
                    username: "friend".into(),
                    password: "account password 123".into(),
                },
                tx,
            )
            .unwrap();
        server.handle(owner, Request::Join { call: room }).unwrap();
        server.handle(friend, Request::Join { call: room }).unwrap();
        assert_eq!(
            server.snapshot(friend).unwrap().voice_rooms[0].participants,
            2
        );
        server
            .handle(
                owner,
                Request::Controls {
                    mic_muted: true,
                    deafened: false,
                },
            )
            .unwrap();
        let mut audio = room.to_be_bytes().to_vec();
        audio.extend([0; 10]);
        assert!(server.relay(owner, &audio).is_empty());
        server.handle(owner, Request::RoomDelete { room }).unwrap();
        assert!(server.snapshot(friend).unwrap().calls.is_empty());
        assert!(server.snapshot(friend).unwrap().voice_rooms.is_empty());
        assert!(server.handle(friend, Request::Join { call: room }).is_err());
        drop(server);
        assert!(Server::open(path, 4434)
            .unwrap()
            .snapshot(owner)
            .unwrap()
            .voice_rooms
            .is_empty());
    }
    #[test]
    fn room_capacity_controls_and_last_leave_keep_room_alive() {
        let mut server = Server::open(":memory:", 4434).unwrap();
        let (owner, _) = account(&mut server, "judas");
        create(&mut server, owner, "Lounge", "231939").unwrap();
        let room = server.snapshot(owner).unwrap().voice_rooms[0].id;
        server.handle(owner, Request::Join { call: room }).unwrap();
        for i in 0..8 {
            let name = format!("friend{i}");
            let (user, _) = account(&mut server, &name);
            server
                .handle(owner, Request::FriendRequest { username: name })
                .unwrap();
            server
                .handle(user, Request::FriendAccept { user: owner })
                .unwrap();
            let result = server.handle(user, Request::Join { call: room });
            if i < 7 {
                result.unwrap();
            } else {
                assert!(result.is_err());
            }
        }
        assert_eq!(
            server.snapshot(owner).unwrap().voice_rooms[0].participants,
            8
        );
        server.handle(owner, Request::Toggle {}).unwrap();
        assert!(server.snapshot(owner).unwrap().calls[0]
            .participants
            .iter()
            .filter(|p| p.user.id != owner)
            .all(|p| p.labeled));
        let users: Vec<_> = server.rooms[&room].members.keys().copied().collect();
        for user in users {
            server.handle(user, Request::Leave {}).unwrap();
        }
        assert_eq!(
            server.snapshot(owner).unwrap().voice_rooms[0].participants,
            0
        );
        server.handle(owner, Request::Join { call: room }).unwrap();
        assert!(server.snapshot(owner).unwrap().calls[0]
            .participants
            .iter()
            .all(|p| !p.labeled));
    }
}
