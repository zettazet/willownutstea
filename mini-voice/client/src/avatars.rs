//! Only visible profiles are requested. At most twenty 32x32 BGRA thumbnails.
use mini_shared::{User, MAX_AVATAR_STORED};
use std::collections::{HashMap, HashSet};
const CAPACITY: usize = 20;
pub struct Thumbnail {
    pub pixels: Vec<u8>,
}
#[derive(Default)]
pub struct Cache {
    wanted: HashMap<i64, String>,
    images: HashMap<i64, Thumbnail>,
    pending: HashSet<i64>,
}
impl Cache {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn retry(&mut self) {
        self.pending.clear();
    }
    pub fn reconcile(&mut self, enabled: bool, users: impl IntoIterator<Item = User>) -> Vec<i64> {
        if !enabled {
            self.clear();
            return vec![];
        }
        let mut wanted = HashMap::new();
        for user in users {
            if wanted.len() == CAPACITY {
                break;
            }
            if let Some(revision) = user.avatar {
                wanted.insert(user.id, revision);
            }
        }
        self.images
            .retain(|id, _| wanted.get(id) == self.wanted.get(id));
        self.pending
            .retain(|id| wanted.get(id) == self.wanted.get(id));
        self.wanted = wanted;
        let requests: Vec<_> = self
            .wanted
            .keys()
            .filter(|id| !self.images.contains_key(id) && !self.pending.contains(id))
            .copied()
            .collect();
        self.pending.extend(requests.iter().copied());
        requests
    }
    pub fn failed(&mut self, id: i64) {
        self.pending.remove(&id);
    }
    pub fn accept(
        &mut self,
        enabled: bool,
        user: i64,
        revision: Option<&str>,
        data: &str,
    ) -> anyhow::Result<bool> {
        if !enabled
            || self.wanted.get(&user).map(String::as_str) != revision
            || !self.pending.remove(&user)
        {
            return Ok(false);
        }
        anyhow::ensure!(
            data.len() <= MAX_AVATAR_STORED * 2,
            "Avatar response too large"
        );
        let bytes = hex::decode(data)?;
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Jpeg);
        let (width, height) = reader.into_dimensions()?;
        anyhow::ensure!(
            width > 0 && height > 0 && width <= 256 && height <= 256,
            "Invalid avatar dimensions"
        );
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)?;
        let small = decoded
            .resize_to_fill(32, 32, image::imageops::FilterType::Triangle)
            .to_rgba8();
        let mut pixels = small.into_raw();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        self.images.insert(user, Thumbnail { pixels });
        Ok(true)
    }
    pub fn get(&self, id: i64) -> Option<&Thumbnail> {
        self.images.get(&id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn user(id: i64) -> User {
        User {
            id,
            username: format!("user{id}"),
            avatar: Some(format!("revision{id}")),
        }
    }
    #[test]
    fn cache_disabled_evicts_and_does_not_decode_or_request() {
        let mut cache = Cache::default();
        let requests = cache.reconcile(true, (0..100).map(user));
        assert_eq!(requests.len(), CAPACITY);
        let image = image::DynamicImage::new_rgb8(256, 256);
        let mut data = std::io::Cursor::new(Vec::new());
        image.write_to(&mut data, image::ImageFormat::Jpeg).unwrap();
        assert!(cache
            .accept(true, 0, Some("revision0"), &hex::encode(data.into_inner()))
            .unwrap());
        assert_eq!(cache.get(0).unwrap().pixels.len(), 4096);
        assert!(cache.reconcile(true, (0..100).map(user)).is_empty());
        assert!(cache.reconcile(false, (0..100).map(user)).is_empty());
        assert!(cache.images.is_empty() && cache.pending.is_empty());
        assert!(!cache
            .accept(
                false,
                0,
                Some("revision0"),
                "invalid data must not be decoded"
            )
            .unwrap());
        assert_eq!(cache.reconcile(true, [user(0)]), vec![0]);
        let mut changed = user(0);
        changed.avatar = Some("new".into());
        assert_eq!(cache.reconcile(true, [changed]), vec![0]);
        assert!(!cache
            .accept(true, 0, Some("revision0"), "invalid stale response")
            .unwrap());
    }
}
