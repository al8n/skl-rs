use core::cmp;

use super::*;

mod iterator;
pub use iterator::*;

/// A Snapshot is a read-only view of the SkipMap at a specific point in time.
/// It provides a consistent view of the SkipMap's contents, which can be useful
/// for various purposes, such as implementing transactional semantics.
pub struct Snapshot<K: Key, V: Value, C: Comparator> {
  map: SkipMap<K, V, C>,
  height: u32,
  len: u32,
  size: usize,
  last: NodePtr<K::Trailer, V::Trailer>,
}

impl<K: Key, V: Value, C: Comparator> Snapshot<K, V, C> {
  /// Returns the height of the highest tower within any of the nodes that
  /// have ever been allocated as part of this skiplist.
  #[inline]
  pub const fn height(&self) -> u32 {
    self.height
  }

  /// Returns the arena backing this skipmap.
  #[inline]
  pub const fn arena(&self) -> &Arena {
    self.map.arena()
  }

  /// Returns the number of bytes that have allocated from the arena.
  #[inline]
  pub const fn size(&self) -> usize {
    self.size
  }

  /// Returns the number of entries in the skipmap.
  #[inline]
  pub const fn len(&self) -> usize {
    self.len as usize
  }

  /// Returns true if the skipmap is empty.
  #[inline]
  pub const fn is_empty(&self) -> bool {
    self.len == 0
  }

  /// Returns the first entry in the skipmap.
  #[inline]
  pub fn first(&self) -> Option<EntryRef<K, V, C>> {
    self.map.first()
  }

  /// Returns the last entry in the skipmap.
  #[inline]
  pub fn last(&self) -> Option<EntryRef<K, V, C>> {
    if self.len == 0 {
      return None;
    }
    unsafe {
      let last_node = self.last.as_ptr();

      Some(EntryRef {
        map: &self.map,
        nd: self.last,
        key: KeyRef::new(last_node.get_key(self.map.arena()), last_node.key_trailer),
        value: ValueRef::new(
          last_node.get_value(self.map.arena()),
          last_node.value_trailer,
        ),
      })
    }
  }

  /// Returns the `i`-th entry in the skip map.
  /// This operation is `O(2/n)`.
  pub fn ith(&self, index: usize) -> Option<EntryRef<K, V, C>> {
    let len = self.len as usize;
    if index >= len {
      return None;
    }
    unsafe {
      // located in the first half
      let nd = if index < len / 2 {
        let mut nd = self.map.head;
        for _ in 0..index {
          nd = self.map.get_next(nd, 0);
        }
        nd
      } else {
        let mut nd = self.last;
        for _ in 0..index {
          nd = self.map.get_prev(nd, 0);
        }
        nd
      };

      let node = nd.as_ptr();
      Some(EntryRef {
        map: &self.map,
        nd,
        key: KeyRef::new(node.get_key(&self.map.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.map.arena), node.value_trailer),
      })
    }
  }

  /// Returns true if the key exists in the map.
  pub fn contains_key<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> bool
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    let last_key = self.last_key();
    let kr = key.as_key_ref();
    match self.map.cmp.compare(kr.as_bytes(), last_key.as_bytes()) {
      cmp::Ordering::Less => self.map.contains_key(key),
      cmp::Ordering::Equal => match kr.trailer().cmp(last_key.trailer()) {
        cmp::Ordering::Less => self.map.contains_key(key),
        cmp::Ordering::Equal => true,
        cmp::Ordering::Greater => false,
      },
      cmp::Ordering::Greater => false,
    }
  }

  /// Returns the value associated with the given key, if it exists.
  pub fn get<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<ValueRef<'a, V>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    let last_key = self.last_key();
    let kr = key.as_key_ref();
    match self.map.cmp.compare(kr.as_bytes(), last_key.as_bytes()) {
      cmp::Ordering::Less => self.map.get(key),
      cmp::Ordering::Equal => match kr.trailer().cmp(last_key.trailer()) {
        cmp::Ordering::Less => self.map.get(key),
        cmp::Ordering::Equal => Some(self.last_value()),
        cmp::Ordering::Greater => None,
      },
      cmp::Ordering::Greater => None,
    }
  }
}

impl<K: Key, V: Value, C: Comparator> Snapshot<K, V, C> {
  pub(super) fn new(
    map: SkipMap<K, V, C>,
    height: u32,
    len: u32,
    size: usize,
    last: NodePtr<K::Trailer, V::Trailer>,
  ) -> Self {
    Self {
      map,
      height,
      len,
      size,
      last,
    }
  }

  #[inline]
  fn last_key(&self) -> KeyRef<K> {
    unsafe {
      let last_node = self.last.as_ptr();
      KeyRef::new(last_node.get_key(self.map.arena()), last_node.key_trailer)
    }
  }

  #[inline]
  fn last_value(&self) -> ValueRef<V> {
    unsafe {
      let last_node = self.last.as_ptr();
      ValueRef::new(
        last_node.get_value(self.map.arena()),
        last_node.value_trailer,
      )
    }
  }
}
