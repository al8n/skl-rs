use crate::ArenaError;

use super::{key::AsKeyRef, Arena, Comparator, Key, SkipMap, Value};

mod error;
pub use error::Error;
mod entry;
pub use entry::EntryRef;

mod iterator;
pub use iterator::*;

struct VoidValue;

impl Value for VoidValue {
  type Trailer = ();

  fn as_bytes(&self) -> &[u8] {
    &[]
  }

  fn trailer(&self) -> &Self::Trailer {
    &()
  }
}

/// A fast, cocnurrent set implementation based on skiplist that supports forward
/// and backward iteration. Keys are immutable once added to the skipmap and
/// deletion is not supported. Instead, higher-level code is expected to add new
/// entries that shadow existing entries and perform deletion via tombstones. It
/// is up to the user to process these shadow entries and tombstones
/// appropriately during retrieval.
#[repr(transparent)]
pub struct SkipSet<K: Key, C: Comparator = ()> {
  map: SkipMap<K, VoidValue, C>,
}

impl<K: Key, C: Comparator> SkipSet<K, C> {
  /// Returns the height of the highest tower within any of the nodes that
  /// have ever been allocated as part of this skiplist.
  #[inline]
  pub fn height(&self) -> u32 {
    self.map.height()
  }

  /// Returns the arena backing this skipmap.
  #[inline]
  pub fn arena(&self) -> &Arena {
    self.map.arena()
  }

  /// Returns the number of bytes that have allocated from the arena.
  #[inline]
  pub fn size(&self) -> usize {
    self.map.size()
  }

  /// Returns the number of entries in the skipmap.
  #[inline]
  pub fn len(&self) -> usize {
    self.map.len()
  }

  /// Returns true if the skipmap is empty.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.map.is_empty()
  }
}

impl<K: Key> SkipSet<K, ()> {
  /// Create a new skipset according to the given capacity
  ///
  /// **Note:** The capacity stands for how many memory allocated,
  /// it does not mean the skiplist can store `cap` entries.
  ///
  ///
  ///
  /// **What the difference between this method and [`SkipSet::mmap_anon`]?**
  ///
  /// 1. This method will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///   Even if we are working with raw pointers with `Box::into_raw`,
  ///   the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///   when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///   especially if you're frequently accessing or modifying it.
  ///
  /// 2. Where as [`SkipSet::mmap_anon`] will use mmap anonymous to require memory from the OS.
  ///   If you require very large contiguous memory regions, `mmap` might be more suitable because
  ///   it's more direct in requesting large chunks of memory from the OS.
  ///
  /// [`SkipSet::mmap_anon`]: #method.mmap_anon
  pub fn new(cap: usize) -> Result<Self, Error> {
    SkipMap::<K, VoidValue>::new(cap)
      .map(|map| Self { map })
      .map_err(|_| Error::Full(ArenaError))
  }

  /// Create a new skipset according to the given capacity, and mmaped to a file.
  ///
  /// **Note:** The capacity stands for how many memory mmaped,
  /// it does not mean the skipset can store `cap` entries.
  ///
  /// `lock`: whether to lock the underlying file or not
  #[cfg(feature = "mmap")]
  #[cfg_attr(docsrs, doc(cfg(feature = "mmap")))]
  pub fn mmap(cap: usize, file: std::fs::File, lock: bool) -> std::io::Result<Self> {
    SkipMap::<K, VoidValue>::mmap(cap, file, lock).map(|map| Self { map })
  }

  /// Create a new skipset according to the given capacity, and mmap anon.
  ///
  /// **What the difference between this method and [`SkipSet::new`]?**
  ///
  /// 1. This method will use mmap anonymous to require memory from the OS directly.
  ///   If you require very large contiguous memory regions, this method might be more suitable because
  ///   it's more direct in requesting large chunks of memory from the OS.
  ///
  /// 2. Where as [`SkipSet::new`] will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///   Even if we are working with raw pointers with `Box::into_raw`,
  ///   the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///   when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///   especially if you're frequently accessing or modifying it.
  ///
  /// [`SkipSet::new`]: #method.new
  #[cfg(feature = "mmap")]
  #[cfg_attr(docsrs, doc(cfg(feature = "mmap")))]
  pub fn mmap_anon(cap: usize) -> std::io::Result<Self> {
    SkipMap::<K, VoidValue>::mmap_anon(cap).map(|map| Self { map })
  }
}

impl<K: Key, C: Comparator> SkipSet<K, C> {
  /// Returns the first entry in the set.
  pub fn first(&self) -> Option<EntryRef<K, C>> {
    self.map.first().map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the last entry in the set.
  pub fn last(&self) -> Option<EntryRef<K, C>> {
    self.map.last().map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the `i`-th entry in the skip set.
  /// This operation is `O(2/n)`.
  pub fn ith(&self, index: usize) -> Option<EntryRef<K, C>> {
    self.map.ith(index).map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the entry less than the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  pub fn lt<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, C>>
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.map.lt(key).map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the entry less than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k3 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  pub fn le<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, C>>
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.map.le(key).map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the entry greater than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k1, then the entry contains k1 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  pub fn ge<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, C>>
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.map.ge(key).map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns the entry greater than the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k1, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  pub fn gt<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, C>>
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.map.gt(key).map(|ent| EntryRef {
      map: &self.map,
      nd: ent.ptr(),
      key: *ent.key(),
    })
  }

  /// Returns true if the key exists in the set.
  #[inline]
  pub fn contains<'a: 'b, 'b, Q>(&'b self, key: &'a Q) -> bool
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.map.contains_key(key)
  }

  /// Inserts a new key if it does not yet exist.
  ///
  /// - Returns `Ok(false)` if the key was successfully inserted.
  /// - Returns `Ok(true)` if the key was already exist.
  ///
  /// As a low-level crate, users are expected to handle the error cases themselves.
  ///
  /// # Errors
  ///
  /// - Returns `Error::Full`, if there isn't enough room in the arena.
  pub fn insert<'a, 'b: 'a, Q, R>(&'a self, key: &'b Q) -> Result<bool, Error>
  where
    K::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    match self.map.insert(key, &VoidValue) {
      Ok(_) => Ok(false),
      Err(e) => match e {
        super::map::Error::Full(e) => Err(Error::Full(e)),
        super::map::Error::Duplicated => Ok(true),
      },
    }
  }

  /// Returns a new `Iterator`. Note that it is
  /// safe for an iterator to be copied by value.
  #[inline]
  pub const fn iter(&self) -> iterator::SetIterator<K, K, K, C> {
    iterator::SetIterator::new(self)
  }

  /// Returns a new `Iterator` that with the lower and upper bounds.
  ///
  /// Returns `None` if `lower` is greater than or equal to `upper`.
  #[inline]
  pub fn iter_bounded<'a, L, U>(
    &'a self,
    lower: L,
    upper: U,
  ) -> Option<iterator::SetIterator<'a, K, L, U, C>>
  where
    L: AsKeyRef<Key = K> + 'a,
    U: AsKeyRef<Key = K> + 'a,
  {
    iterator::SetIterator::bounded(self, lower, upper)
  }

  /// Returns a new `Iterator` that with the lower bound.
  #[inline]
  pub const fn iter_bound_lower<'a, L>(&'a self, lower: L) -> iterator::SetIterator<'a, K, L, K, C>
  where
    L: AsKeyRef<Key = K> + 'a,
  {
    iterator::SetIterator::bound_lower(self, lower)
  }

  /// Returns a new `Iterator` that with the upper bound.
  #[inline]
  pub const fn iter_bound_upper<'a, U>(&'a self, upper: U) -> iterator::SetIterator<'a, K, K, U, C>
  where
    U: AsKeyRef<Key = K> + 'a,
  {
    iterator::SetIterator::bound_upper(self, upper)
  }
}
