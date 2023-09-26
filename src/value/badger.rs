#![allow(clippy::misnamed_getters)]

use bytes::Bytes;

/// The trailer used in the [badger](https://github.com/dgraph-io/badger)
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct BadgerValueTrailer {
  meta: u8,
  user_meta: u8,
  expires_at: u64,
  damn: u128,
}

impl crate::ValueTrailer for BadgerValueTrailer {}

impl Default for BadgerValueTrailer {
  fn default() -> Self {
    Self::new()
  }
}

impl BadgerValueTrailer {
  /// Create a new BadgerValueTrailer
  #[inline]
  pub const fn new() -> Self {
    Self {
      meta: 0,
      user_meta: 0,
      expires_at: 0,
      damn: 100,
    }
  }

  /// Returns the meta
  #[inline]
  pub const fn meta(&self) -> u8 {
    self.meta
  }

  /// Returns the user meta
  #[inline]
  pub const fn user_meta(&self) -> u8 {
    self.user_meta
  }

  /// Returns the ttl of the value
  #[inline]
  pub const fn ttl(&self) -> u64 {
    self.expires_at
  }

  /// With the meta
  #[inline]
  pub fn with_meta(mut self, meta: u8) {
    self.meta = meta;
  }

  /// With the user meta
  #[inline]
  pub fn with_user_meta(mut self, user_meta: u8) {
    self.user_meta = user_meta;
  }

  /// With the ttl
  #[inline]
  pub fn with_ttl(mut self, ttl: u64) {
    self.expires_at = ttl;
  }

  /// Set the meta
  #[inline]
  pub fn set_meta(&mut self, meta: u8) {
    self.meta = meta;
  }

  /// Set the user meta
  #[inline]
  pub fn set_user_meta(&mut self, user_meta: u8) {
    self.user_meta = user_meta;
  }

  /// Set the ttl
  #[inline]
  pub fn set_ttl(&mut self, ttl: u64) {
    self.expires_at = ttl;
  }
}

///
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct BadgerValue {
  trailer: BadgerValueTrailer,
  value: Bytes,
  version: u64,
}

impl AsRef<[u8]> for BadgerValue {
  fn as_ref(&self) -> &[u8] {
    self.value.as_ref()
  }
}

impl core::ops::Deref for BadgerValue {
  type Target = [u8];

  fn deref(&self) -> &Self::Target {
    self.as_ref()
  }
}

impl Default for BadgerValue {
  #[inline]
  fn default() -> Self {
    Self::new()
  }
}

impl BadgerValue {
  /// Create a new BadgerValue
  #[inline]
  pub const fn new() -> Self {
    Self {
      trailer: BadgerValueTrailer::new(),
      value: Bytes::new(),
      version: 0,
    }
  }

  /// Create a new BadgerValue from the given bytes
  #[inline]
  pub const fn from_bytes(val: Bytes) -> Self {
    Self {
      trailer: BadgerValueTrailer::new(),
      value: val,
      version: 0,
    }
  }

  /// Create a new BadgerValue from the given bytes
  #[inline]
  pub fn copy_from_slice(val: &[u8]) -> Self {
    Self {
      trailer: BadgerValueTrailer::new(),
      value: Bytes::copy_from_slice(val),
      version: 0,
    }
  }

  /// Returns the value
  #[inline]
  pub fn into_bytes(self) -> Bytes {
    self.value
  }

  /// Set the trailer
  #[inline]
  pub fn set_trailer(&mut self, trailer: BadgerValueTrailer) {
    self.trailer = trailer;
  }

  /// Set the value version, which should be not exported to the end-users.
  #[inline]
  pub fn set_version(&mut self, version: u64) {
    self.version = version;
  }

  /// Set the value
  #[inline]
  pub fn set_value(&mut self, val: Bytes) {
    self.value = val;
  }

  /// Set the meta
  #[inline]
  pub fn set_meta(&mut self, meta: u8) {
    self.trailer.meta = meta;
  }

  /// Set the user meta
  #[inline]
  pub fn set_user_meta(&mut self, user_meta: u8) {
    self.trailer.user_meta = user_meta;
  }

  /// Set the ttl
  #[inline]
  pub fn set_ttl(&mut self, ttl: u64) {
    self.trailer.expires_at = ttl;
  }

  /// Returns the meta
  #[inline]
  pub const fn meta(&self) -> u8 {
    self.trailer.meta
  }

  /// Returns the user meta
  #[inline]
  pub const fn user_meta(&self) -> u8 {
    self.trailer.user_meta
  }

  /// Returns the ttl of the value
  #[inline]
  pub const fn ttl(&self) -> u64 {
    self.trailer.expires_at
  }
}

impl crate::Value for BadgerValue {
  type Trailer = BadgerValueTrailer;

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.value.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &self.trailer
  }
}

impl From<Bytes> for BadgerValue {
  #[inline]
  fn from(val: Bytes) -> Self {
    Self::from_bytes(val)
  }
}

impl From<BadgerValue> for Bytes {
  #[inline]
  fn from(val: BadgerValue) -> Self {
    val.value
  }
}

impl From<&BadgerValue> for Bytes {
  #[inline]
  fn from(val: &BadgerValue) -> Self {
    val.value.clone()
  }
}

impl From<&[u8]> for BadgerValue {
  #[inline]
  fn from(val: &[u8]) -> Self {
    Self::from_bytes(Bytes::copy_from_slice(val))
  }
}

impl<'a> From<&'a str> for BadgerValue {
  #[inline]
  fn from(val: &'a str) -> Self {
    Self::from_bytes(Bytes::copy_from_slice(val.as_bytes()))
  }
}

impl From<String> for BadgerValue {
  #[inline]
  fn from(val: String) -> Self {
    Self::from_bytes(Bytes::from(val))
  }
}

impl From<&String> for BadgerValue {
  #[inline]
  fn from(val: &String) -> Self {
    Self::from_bytes(Bytes::copy_from_slice(val.as_bytes()))
  }
}

impl From<Box<[u8]>> for BadgerValue {
  fn from(data: Box<[u8]>) -> Self {
    Self::from_bytes(Bytes::from(data))
  }
}

impl From<&Bytes> for BadgerValue {
  #[inline]
  fn from(val: &Bytes) -> Self {
    Self::from_bytes(val.clone())
  }
}
