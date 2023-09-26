use bytes::Bytes;

/// The Key type used in [badger](https://github.com/dgraph-io/badger).
#[derive(Debug, Clone)]
pub struct BadgerKey {
  expires_at: u64,
  data: Bytes,
}

impl crate::Key for BadgerKey {
  type Trailer = u64;

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.data.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &self.expires_at
  }
}

impl AsRef<[u8]> for BadgerKey {
  fn as_ref(&self) -> &[u8] {
    self.data.as_ref()
  }
}

impl core::ops::Deref for BadgerKey {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    self.as_ref()
  }
}

impl PartialEq for BadgerKey {
  fn eq(&self, other: &Self) -> bool {
    self.data == other.data && self.expires_at == other.expires_at
  }
}

impl Eq for BadgerKey {}

impl core::hash::Hash for BadgerKey {
  fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
    self.data.hash(state);
  }
}

impl PartialOrd for BadgerKey {
  fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for BadgerKey {
  fn cmp(&self, other: &Self) -> core::cmp::Ordering {
    self
      .data
      .cmp(&other.data)
      .then_with(|| self.expires_at.cmp(&other.expires_at))
  }
}

impl Default for BadgerKey {
  fn default() -> Self {
    Self::new()
  }
}

impl BadgerKey {
  /// Returns an empty key
  #[inline]
  pub const fn new() -> Self {
    Self {
      data: Bytes::new(),
      expires_at: 0,
    }
  }

  /// Set the version of the key
  #[inline]
  pub const fn with_version(mut self, version: u64) -> Self {
    self.expires_at = version;
    self
  }

  /// Create a key from bytes, without version
  #[inline]
  pub const fn from_bytes(b: Bytes) -> Self {
    Self {
      data: b,
      expires_at: 0,
    }
  }

  /// Destruct the key, returns key and ttl of the key.
  #[inline]
  pub fn into_components(self) -> (Bytes, u64) {
    (self.data, self.expires_at)
  }

  /// Set the version of the key
  #[inline]
  pub fn set_version(&mut self, ttl: u64) {
    self.expires_at = ttl;
  }

  /// Returns a key, with the current timestamp (in milliseconds) as version
  #[cfg(feature = "std")]
  #[cfg_attr(docsrs, doc(cfg(feature = "std")))]
  pub fn with_now(src: Bytes) -> Self {
    Self {
      data: src,
      expires_at: std::time::SystemTime::UNIX_EPOCH
        .elapsed()
        .unwrap()
        .as_millis() as u64,
    }
  }
}

impl From<Bytes> for BadgerKey {
  fn from(data: Bytes) -> Self {
    Self {
      data,
      expires_at: 0,
    }
  }
}

impl<'a> From<&'a [u8]> for BadgerKey {
  fn from(data: &'a [u8]) -> Self {
    Self {
      data: Bytes::copy_from_slice(data),
      expires_at: 0,
    }
  }
}

impl<'a> From<&'a str> for BadgerKey {
  fn from(data: &str) -> Self {
    Self {
      data: Bytes::copy_from_slice(data.as_bytes()),
      expires_at: 0,
    }
  }
}

impl From<String> for BadgerKey {
  fn from(data: String) -> Self {
    Self {
      data: data.into(),
      expires_at: 0,
    }
  }
}

impl From<&BadgerKey> for BadgerKey {
  fn from(key: &BadgerKey) -> Self {
    Self {
      data: key.data.clone(),
      expires_at: key.expires_at,
    }
  }
}

impl From<BadgerKey> for Bytes {
  fn from(key: BadgerKey) -> Self {
    key.data
  }
}

impl From<&BadgerKey> for Bytes {
  fn from(key: &BadgerKey) -> Self {
    key.data.clone()
  }
}

impl From<Vec<u8>> for BadgerKey {
  fn from(data: Vec<u8>) -> Self {
    Self {
      data: data.into(),
      expires_at: 0,
    }
  }
}

impl From<Box<[u8]>> for BadgerKey {
  fn from(data: Box<[u8]>) -> Self {
    Self {
      data: data.into(),
      expires_at: 0,
    }
  }
}

impl From<&Bytes> for BadgerKey {
  fn from(data: &Bytes) -> Self {
    Self {
      data: data.clone(),
      expires_at: 0,
    }
  }
}
