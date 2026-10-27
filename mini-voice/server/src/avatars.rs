use super::*;
use image::{ImageFormat, ImageReader};
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
pub(super) struct Upload {
    total: usize,
    data: Vec<u8>,
    started: Instant,
}
/// Decode only supported formats within strict allocation and pixel limits.
pub fn optimize(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_AVATAR_UPLOAD,
        "Avatar exceeds 5 MiB"
    );
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
        ),
        "Use PNG, JPEG or WebP"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits.clone());
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 4 * 1024 * 1024,
        "Avatar dimensions exceed 4 megapixels"
    );
    let decoded = reader.decode()?;
    let small = decoded.thumbnail(256, 256).to_rgba8();
    let mut rgb = image::RgbImage::new(small.width(), small.height());
    for (source, dest) in small.pixels().zip(rgb.pixels_mut()) {
        let alpha = u32::from(source[3]);
        for i in 0..3 {
            dest[i] = ((u32::from(source[i]) * alpha + 32 * (255 - alpha)) / 255) as u8;
        }
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&rgb)?;
    ensure!(out.len() <= MAX_AVATAR_STORED, "Optimized avatar too large");
    Ok(out)
}
impl Server {
    pub(super) fn expire_avatar_uploads(&mut self) {
        self.uploads
            .retain(|_, upload| upload.started.elapsed() < Duration::from_secs(60));
    }
    pub(super) fn avatar_chunk(
        &mut self,
        u: i64,
        total: usize,
        offset: usize,
        data: String,
    ) -> Result<()> {
        if let Some(bytes) = self.avatar_chunk_work(u, total, offset, data)? {
            self.save_avatar(u, optimize(&bytes)?)?;
        }
        Ok(())
    }
    pub fn avatar_chunk_work(
        &mut self,
        u: i64,
        total: usize,
        offset: usize,
        data: String,
    ) -> Result<Option<Vec<u8>>> {
        self.expire_avatar_uploads();
        ensure!(
            total > 0 && total <= MAX_AVATAR_UPLOAD && data.len() <= AVATAR_CHUNK * 2,
            "Invalid avatar upload size"
        );
        let bytes = hex::decode(data)?;
        ensure!(!bytes.is_empty(), "Empty avatar chunk");
        if offset == 0 {
            ensure!(
                self.uploads.contains_key(&u) || self.uploads.len() < 2,
                "Avatar uploads busy; try again"
            );
            if let Some(peer) = self.peers.get_mut(&u) {
                peer.avatar_epoch = peer.avatar_epoch.wrapping_add(1);
            }
            self.uploads.insert(
                u,
                Upload {
                    total,
                    data: Vec::with_capacity(total),
                    started: Instant::now(),
                },
            );
        }
        let upload = self
            .uploads
            .get_mut(&u)
            .ok_or_else(|| anyhow::anyhow!("Restart avatar upload"))?;
        if total != upload.total || offset != upload.data.len() || offset + bytes.len() > total {
            self.uploads.remove(&u);
            bail!("Invalid avatar chunk order");
        }
        upload.data.extend_from_slice(&bytes);
        upload.started = Instant::now();
        if upload.data.len() == total {
            let upload = self.uploads.remove(&u).unwrap();
            return Ok(Some(upload.data));
        }
        Ok(None)
    }
    pub fn save_avatar(&mut self, u: i64, data: Vec<u8>) -> Result<()> {
        let revision = hex::encode(Sha256::digest(&data));
        self.db.execute("INSERT INTO avatars(user,revision,data) VALUES(?,?,?) ON CONFLICT(user) DO UPDATE SET revision=excluded.revision,data=excluded.data",params![u,revision,data])?;
        self.emit(
            u,
            Event::AvatarSaved {
                revision: Some(revision),
            },
        );
        self.refresh();
        Ok(())
    }
    pub(super) fn avatar_get(&self, viewer: i64, user: i64) -> Result<()> {
        let allowed=viewer==user || self.friend(viewer,user) || self.rooms.values().any(|room|room.members.contains_key(&viewer)&&room.members.contains_key(&user)) || self.db.query_row("SELECT EXISTS(SELECT 1 FROM members a JOIN members b ON a.conversation=b.conversation WHERE a.user=? AND b.user=?)",params![viewer,user],|r|r.get::<_,bool>(0))?;
        ensure!(allowed, "Profile access denied");
        let row: Option<(String, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT revision,data FROM avatars WHERE user=?",
                [user],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (revision, data) = row
            .map(|(r, d)| (Some(r), hex::encode(d)))
            .unwrap_or_default();
        self.emit(
            viewer,
            Event::Avatar {
                user,
                revision,
                data,
            },
        );
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_validation_and_bounded_normalization() {
        let source = image::DynamicImage::new_rgba8(600, 300);
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let mut file = Cursor::new(Vec::new());
            source.to_rgb8().write_to(&mut file, format).unwrap();
            let normalized = optimize(file.get_ref()).unwrap();
            let result = image::load_from_memory(&normalized).unwrap();
            assert_eq!((result.width(), result.height()), (256, 128));
            assert!(normalized.len() <= MAX_AVATAR_STORED);
        }
        assert!(optimize(b"not an image").is_err());
        assert!(optimize(&vec![0; MAX_AVATAR_UPLOAD + 1]).is_err());
        let mut file = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4097, 1)
            .write_to(&mut file, ImageFormat::Png)
            .unwrap();
        assert!(optimize(file.get_ref()).is_err());
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn chunk_permissions_replacement_removal_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.sqlite");
        let mut server = Server::open(path.to_str().unwrap(), 4434).unwrap();
        let (tx, mut rx) = mpsc::channel(128);
        let (user, _) = server
            .auth_verified(
                VerifiedAuth::Register("avatar_user".into(), "unused".into()),
                tx,
            )
            .unwrap();
        let (tx, _rx) = mpsc::channel(128);
        let (other, _) = server
            .auth_verified(
                VerifiedAuth::Register("unrelated".into(), "unused".into()),
                tx,
            )
            .unwrap();
        assert!(server
            .handle(
                user,
                Request::AvatarChunk {
                    total: MAX_AVATAR_UPLOAD + 1,
                    offset: 0,
                    data: "00".into()
                }
            )
            .is_err());
        assert!(server
            .handle(
                user,
                Request::AvatarChunk {
                    total: 2,
                    offset: 1,
                    data: "00".into()
                }
            )
            .is_err());
        let image = image::RgbImage::from_fn(300, 300, |x, y| {
            image::Rgb([(x * y) as u8, x as u8, y as u8])
        });
        let mut file = Cursor::new(Vec::new());
        image.write_to(&mut file, ImageFormat::Png).unwrap();
        let bytes = file.into_inner();
        assert!(bytes.len() > AVATAR_CHUNK);
        for (i, chunk) in bytes.chunks(AVATAR_CHUNK).enumerate() {
            server
                .handle(
                    user,
                    Request::AvatarChunk {
                        total: bytes.len(),
                        offset: i * AVATAR_CHUNK,
                        data: hex::encode(chunk),
                    },
                )
                .unwrap();
        }
        let revision = server.user(user).unwrap().avatar.unwrap();
        assert!(server.handle(other, Request::AvatarGet { user }).is_err());
        server.rooms.insert(
            1,
            Room {
                id: 1,
                conversation: 0,
                members: HashMap::from([
                    (
                        user,
                        Member {
                            mic: false,
                            deaf: false,
                        },
                    ),
                    (
                        other,
                        Member {
                            mic: false,
                            deaf: false,
                        },
                    ),
                ]),
                invited: HashSet::new(),
                toggles: HashSet::new(),
            },
        );
        assert!(server.handle(other, Request::AvatarGet { user }).is_ok());
        server.rooms.clear();

        server.handle(user, Request::AvatarGet { user }).unwrap();
        let mut received = false;
        while let Ok(event) = rx.try_recv() {
            if let Out::Json(Event::Avatar {
                revision: Some(r),
                data,
                ..
            }) = event
            {
                assert_eq!(r, revision);
                assert!(!data.is_empty());
                received = true;
            }
        }
        assert!(received);
        let count: i64 = server
            .db
            .query_row("SELECT count(*) FROM avatars", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        drop(server);
        let mut server = Server::open(path.to_str().unwrap(), 4434).unwrap();
        assert_eq!(
            server.user(user).unwrap().avatar.as_deref(),
            Some(revision.as_str())
        );
        let (tx, _) = mpsc::channel(128);
        server.auth_verified(VerifiedAuth::Login(user), tx).unwrap();
        let mut replacement = Cursor::new(Vec::new());
        image::RgbImage::new(16, 16)
            .write_to(&mut replacement, ImageFormat::Png)
            .unwrap();
        let bytes = replacement.into_inner();
        server
            .handle(
                user,
                Request::AvatarChunk {
                    total: bytes.len(),
                    offset: 0,
                    data: hex::encode(bytes),
                },
            )
            .unwrap();
        assert_ne!(
            server.user(user).unwrap().avatar.as_deref(),
            Some(revision.as_str())
        );
        assert_eq!(
            server
                .db
                .query_row("SELECT count(*) FROM avatars", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        server.handle(user, Request::AvatarRemove {}).unwrap();
        assert!(server.user(user).unwrap().avatar.is_none());
        assert_eq!(
            server
                .db
                .query_row("SELECT count(*) FROM avatars", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod upload_limits {
    use super::*;
    #[test]
    fn partial_uploads_are_bounded_expire_and_reject_bad_offsets() {
        let mut server = Server::open(":memory:", 4434).unwrap();
        assert!(server
            .avatar_chunk_work(1, MAX_AVATAR_UPLOAD, 0, "00".into())
            .unwrap()
            .is_none());
        assert!(server
            .avatar_chunk_work(2, MAX_AVATAR_UPLOAD, 0, "00".into())
            .unwrap()
            .is_none());
        assert!(server
            .avatar_chunk_work(3, MAX_AVATAR_UPLOAD, 0, "00".into())
            .is_err());
        assert!(server
            .avatar_chunk_work(1, MAX_AVATAR_UPLOAD, 2, "00".into())
            .is_err());
        assert_eq!(server.uploads.len(), 1);
        server.uploads.get_mut(&2).unwrap().started = Instant::now() - Duration::from_secs(61);
        server.expire_rejoins();
        assert!(server.uploads.is_empty());
    }
}
