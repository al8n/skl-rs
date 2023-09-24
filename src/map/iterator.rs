use core::cmp;

use crate::node::NodePtr;

use super::{AsKeyRef, Comparator, Key, KeyRef, SkipMap, Value, ValueRef};

/// An iterator over the skipmap. The current state of the iterator can be cloned by
/// simply value copying the struct.
pub struct MapIterator<'a, K, V, L, U, C = ()>
where
  K: Key,
  V: Value,
  L: Key<Trailer = K::Trailer> + 'a,
  U: Key<Trailer = K::Trailer> + 'a,
  C: Comparator,
{
  pub(super) map: &'a SkipMap<K, V, C>,
  pub(super) nd: NodePtr<K::Trailer, V::Trailer>,
  pub(super) lower: Option<L>,
  pub(super) upper: Option<U>,
}

impl<'a, K, V, L, U, C> MapIterator<'a, K, V, L, U, C>
where
  K: Key,
  V: Value,
  L: Key<Trailer = K::Trailer> + 'a,
  U: Key<Trailer = K::Trailer> + 'a,
  C: Comparator,
{
  /// Creates a new iterator over the skipmap.
  #[inline]
  pub const fn new(map: &'a SkipMap<K, V, C>) -> Self {
    Self {
      map,
      nd: map.head,
      lower: None,
      upper: None,
    }
  }

  /// Creates a new iterator over the skipmap with the given lower and upper bounds.
  #[inline]
  pub const fn bounded(map: &'a SkipMap<K, V, C>, lower: L, upper: U) -> Self {
    Self {
      map,
      nd: map.head,
      lower: Some(lower),
      upper: Some(upper),
    }
  }

  /// Creates a new iterator over the skipmap with the given lower bound.
  #[inline]
  pub const fn bound_lower(map: &'a SkipMap<K, V, C>, lower: L) -> Self {
    Self {
      map,
      nd: map.head,
      lower: Some(lower),
      upper: None,
    }
  }

  /// Creates a new iterator over the skipmap with the given upper bound.
  #[inline]
  pub const fn bound_upper(map: &'a SkipMap<K, V, C>, upper: U) -> Self {
    Self {
      map,
      nd: map.head,
      lower: None,
      upper: Some(upper),
    }
  }

  /// Seeks position at the first entry in map. Returns the key and value
  /// if the iterator is pointing at a valid entry, and `None` otherwise. Note
  /// that First only checks the upper bound. It is up to the caller to ensure
  /// that key is greater than or equal to the lower bound (e.g. via a call to `seek_ge(lower)`).
  pub fn first(&mut self) -> Option<(KeyRef<K>, ValueRef<V>)> {
    unsafe {
      self.nd = self.map.get_next(self.map.head, 0);
      if self.nd.ptr == self.map.tail.ptr {
        return None;
      }

      if let Some(upper) = &self.upper {
        // Safety: the nd is valid, we already check this on line 113
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Less | cmp::Ordering::Equal =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less | cmp::Ordering::Equal =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }

  /// Seeks position at the last entry in list. Returns the key and value if
  /// the iterator is pointing at a valid entry, and `None` otherwise. Note
  /// that Last only checks the lower bound. It is up to the caller to ensure that
  /// key is less than the upper bound (e.g. via a call to `seek_lt(upper)`).
  pub fn last(&mut self) -> Option<(KeyRef<K>, ValueRef<V>)> {
    unsafe {
      self.nd = self.map.get_prev(self.map.tail, 0);
      if self.nd.ptr == self.map.head.ptr {
        return None;
      }

      if let Some(ref lower) = self.lower {
        // Safety: the nd is valid, we already check this on line 142
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Greater = self.map.cmp.compare(lower.as_bytes(), nk) {
          self.nd = self.map.head;
          return None;
        } else if let cmp::Ordering::Greater = lower.trailer().cmp(&node.key_trailer) {
          self.nd = self.map.head;
          return None;
        }
      }
      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }

  /// Advances to the next position. Returns the key and value if the
  /// iterator is pointing at a valid entry, and `None` otherwise.
  #[allow(clippy::should_implement_trait)]
  pub fn next(&mut self) -> Option<(KeyRef<K>, ValueRef<V>)> {
    unsafe {
      self.nd = self.map.get_next(self.nd, 0);
      if self.nd.ptr == self.map.tail.ptr {
        return None;
      }

      if let Some(upper) = &self.upper {
        // Safety: the nd is valid, we already check this on line 168
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Less | cmp::Ordering::Equal =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less | cmp::Ordering::Equal =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }

  /// Advances to the prev position. Returns the key and value if the
  /// iterator is pointing at a valid entry, and `None` otherwise.
  pub fn prev(&mut self) -> Option<(KeyRef<K>, ValueRef<V>)> {
    unsafe {
      self.nd = self.map.get_prev(self.nd, 0);
      if self.nd.ptr == self.map.head.ptr {
        return None;
      }

      if let Some(ref lower) = self.lower {
        // Safety: the nd is valid, we already check this on line 195
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Greater = self.map.cmp.compare(lower.as_bytes(), nk) {
          self.nd = self.map.head;
          return None;
        } else if let cmp::Ordering::Greater = lower.trailer().cmp(&node.key_trailer) {
          self.nd = self.map.head;
          return None;
        }
      }

      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }

  /// Moves the iterator to the first entry whose key is greater than or
  /// equal to the given key. Returns the key and value if the iterator is
  /// pointing at a valid entry, and (nil, nil) otherwise. Note that `seek_ge` only
  /// checks the upper bound. It is up to the caller to ensure that key is greater
  /// than or equal to the lower bound.
  pub fn seek_ge<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    let res = self.seek_for_base_splice(key.as_key_ref());
    self.nd = res.next;
    if self.nd.ptr == self.map.tail.ptr {
      return None;
    }

    if let Some(ref upper) = self.upper {
      unsafe {
        // Safety: the nd is valid, we already check this on line 75
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Less | cmp::Ordering::Equal =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less | cmp::Ordering::Equal =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }
    }

    // Safety: the nd is valid, we already check this on line 115
    unsafe { Some((self.key_unchecked(), self.value_unchecked())) }
  }

  /// Moves the iterator to the last entry whose key is less than the given
  /// key. Returns the key and value if the iterator is pointing at a valid entry,
  /// and `None` otherwise. Note that `seek_lt` only checks the lower bound. It
  /// is up to the caller to ensure that key is less than the upper bound.
  pub fn seek_lt<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    // NB: the top-level MapIterator has already adjusted key based on
    // the upper-bound.
    let res = self.seek_for_base_splice(key.as_key_ref());
    self.nd = res.prev;
    if self.nd.ptr == self.map.head.ptr {
      return None;
    }

    if let Some(ref lower) = self.lower {
      unsafe {
        // Safety: the nd is valid, we already check this on line 75
        let node = self.nd.as_ptr();
        // Safety: the node is allocated by the map's arena, so the key is valid
        let nk = node.get_key(&self.map.arena);
        if let cmp::Ordering::Greater = self.map.cmp.compare(lower.as_bytes(), nk) {
          self.nd = self.map.head;
          return None;
        } else if let cmp::Ordering::Greater = lower.trailer().cmp(&node.key_trailer) {
          self.nd = self.map.head;
          return None;
        }
      }
    }

    // Safety: the nd is valid, we already check this on line 115
    unsafe { Some((self.key_unchecked(), self.value_unchecked())) }
  }
}

impl<'a, K, V, L, U, C> MapIterator<'a, K, V, L, U, C>
where
  K: Key,
  V: Value,
  L: Key<Trailer = K::Trailer> + 'a,
  U: Key<Trailer = K::Trailer> + 'a,
  C: Comparator,
{
  /// Returns the key of the current entry without any checks.
  ///
  /// ## Safety
  /// - The current node must be valid
  unsafe fn key_unchecked(&self) -> KeyRef<'_, K> {
    let nd = self.nd.as_ptr();
    let val = nd.get_key(&self.map.arena);
    KeyRef::new(val, &nd.key_trailer)
  }

  /// Returns the value of the current entry without any checks.
  ///
  /// ## Safety
  /// - The current node must be valid
  unsafe fn value_unchecked(&self) -> ValueRef<'_, V> {
    let nd = self.nd.as_ptr();
    let val = nd.get_value(&self.map.arena);
    ValueRef::new(val, nd.value_trailer)
  }

  fn seek_for_base_splice(&self, key: KeyRef<K>) -> SeekResult<K, V> {
    let mut lvl = (self.map.height() - 1) as usize;

    let mut prev = self.map.head;
    let mut next;

    loop {
      let fr = unsafe { self.map.find_splice_for_level(&key, lvl, prev) };
      prev = fr.splice.prev;
      next = fr.splice.next;
      if fr.found {
        if lvl != 0 {
          // next is pointing at the target node, but we need to find previous on
          // the bottom level.

          // Safety: the next we use here is got from the find_splice_for_level, so must be allocated by the same arena
          prev = unsafe { self.map.get_prev(next, 0) };
        }
        break;
      }

      if lvl == 0 {
        break;
      }

      lvl -= 1;
    }

    SeekResult { prev, next }
  }
}

struct SeekResult<K: Key, V: Value> {
  prev: NodePtr<K::Trailer, V::Trailer>,
  next: NodePtr<K::Trailer, V::Trailer>,
}
