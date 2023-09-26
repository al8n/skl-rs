use crate::KeyTrailer;

/// Implement [badger's](https://github.com/dgraph-io/badger) key type
pub mod badger;

/// Gives the users the ability to define their own key type, rather than just slice.
///
/// For a key-value database, the key inserted by the end-users will always be encoded to a u8 array.
/// But the key-value database developers are tend to add some extra information
/// to the key provided by the end-users.
/// e.g. ttl, version, tombstones, and etc.
///
/// This trait gives the key-value database developers the ability to add extra information
/// to the key provided by the end-users by associated type [`Trailer`](crate::Trailer).
///
/// # Example
///
/// 1. The [`InternalKey`](https://github.com/cockroachdb/pebble/blob/master/internal/base/internal.go#L171) of [cockroachdb's pebble](https://github.com/cockroachdb/pebble) can be implemented by using this trait as:
///
///     ```rust,no_run
///     #[repr(u64)]
///     enum InternalKeyKind {
///       Delete,
///       Set,
///       Merge,
///       LogData,
///       // ...
///     }
///
///     #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
///     #[repr(transparent)]
///     struct PebbleKeyTrailer(u64);
///
///     impl skl::KeyTrailer for PebbleKeyTrailer {}
///
///     impl PebbleKeyTrailer {
///       fn make_trailer(seq_num: u64, kind: InternalKeyKind) -> u64 {
///         (seq_num << 8) | (kind as u64)
///       }
///
///       fn seq_num(&self) -> u64 {
///         self.0 >> 8
///       }
///     }
///
///     struct PebbleKey {
///       user_key: Vec<u8>,
///       trailer: PebbleKeyTrailer,
///     }
///
///     impl skl::Key for PebbleKey {
///       type Trailer = PebbleKeyTrailer;
///       
///       fn as_bytes(&self) -> &[u8] {
///         self.user_key.as_slice()
///       }
///
///       fn trailer(&self) -> &Self::Trailer {
///         &self.trailer
///       }
///     }
///     ```
///
/// 2. The `Key` of [dgraph's badger](https://github.com/dgraph-io/badger) can be implemented by using this trait as:
///   
///     ```rust,no_run
///
///     struct BadgerKey {
///       timestamp: u64,
///       data: Vec<u8>,
///     }
///   
///     impl skl::Key for BadgerKey {
///       type Trailer = u64;
///
///       fn as_bytes(&self) -> &[u8] {
///         self.data.as_slice()
///       }
///
///       fn trailer(&self) -> &Self::Trailer {
///         &self.timestamp
///       }
///     }
///     ```
pub trait Key {
  /// Extra information should be added for the key provided by end-users.
  type Trailer: KeyTrailer;

  /// Returns the bytes of the key without the trailer.
  fn as_bytes(&self) -> &[u8];

  /// Returns the trailer of the key.
  fn trailer(&self) -> &Self::Trailer;
}

impl Key for Vec<u8> {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_slice()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl Key for Box<[u8]> {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl Key for ::alloc::sync::Arc<[u8]> {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl<const N: usize> Key for [u8; N] {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl<'a> Key for &'a [u8] {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl<'a> Key for &'a str {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    str::as_bytes(self)
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

impl Key for String {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_bytes()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

#[cfg(feature = "bytes")]
impl Key for bytes::Bytes {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_ref()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

#[cfg(feature = "smallvec")]
impl<A: smallvec::Array<Item = u8>> Key for smallvec::SmallVec<A> {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.as_slice()
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

#[cfg(feature = "smol_str")]
impl Key for smol_str::SmolStr {
  type Trailer = ();

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    str::as_bytes(self.as_str())
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

/// The key reference
pub struct KeyRef<'a, K: Key> {
  data: &'a [u8],
  trailer: K::Trailer,
}

impl<'a, K: Key> Clone for KeyRef<'a, K> {
  #[inline]
  fn clone(&self) -> Self {
    *self
  }
}

impl<'a, K: Key> Copy for KeyRef<'a, K> {}

impl<'a, K: Key> KeyRef<'a, K> {
  /// Creates a new key reference directly from the given bytes and trailer.
  #[inline]
  pub const fn new(data: &'a [u8], trailer: K::Trailer) -> Self {
    Self { data, trailer }
  }

  /// Returns the bytes of the key without the trailer.
  #[inline]
  pub const fn as_bytes(&self) -> &[u8] {
    self.data
  }

  /// Returns the trailer of the key.
  #[inline]
  pub const fn trailer(&self) -> &K::Trailer {
    &self.trailer
  }

  /// Returns the encoded size of the key part.
  #[inline]
  pub(crate) fn encoded_size(&self) -> usize {
    self.data.len()
  }
}

impl<'a, K: Key> Key for KeyRef<'a, K> {
  type Trailer = K::Trailer;

  #[inline]
  fn as_bytes(&self) -> &[u8] {
    self.data
  }

  #[inline]
  fn trailer(&self) -> &Self::Trailer {
    &self.trailer
  }
}

/// A trait for types that can be converted to a key reference. [`KeyRef`](crate::KeyRef) is the key used to insert to the skiplist.
pub trait AsKeyRef {
  /// The key type.
  type Key: Key;

  /// Converts the given key to a key reference.
  fn as_key_ref(&self) -> KeyRef<Self::Key>;
}

impl<K: Key> AsKeyRef for K {
  type Key = K;

  #[inline]
  fn as_key_ref(&self) -> KeyRef<K> {
    KeyRef {
      data: self.as_bytes(),
      trailer: *self.trailer(),
    }
  }
}
