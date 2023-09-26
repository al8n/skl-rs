use core::cmp;

use crate::node::NodePtr;

use super::{AsKeyRef, Comparator, Key, KeyRef, SkipMap, Value, ValueRef};

/// An iterator over the skipmap. The current state of the iterator can be cloned by
/// simply value copying the struct.
pub struct MapIterator<'a, K, V, L, U, C = ()>
where
  K: Key,
  V: Value,
  L: AsKeyRef<Key = K> + 'a,
  U: AsKeyRef<Key = K> + 'a,
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
  L: AsKeyRef<Key = K> + 'a,
  U: AsKeyRef<Key = K> + 'a,
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
  pub fn bounded(map: &'a SkipMap<K, V, C>, lower: L, upper: U) -> Option<Self> {
    let lower_ref = lower.as_key_ref();
    let upper_ref = upper.as_key_ref();
    match map
      .cmp
      .compare(lower_ref.as_bytes(), upper_ref.as_bytes())
      .then_with(|| lower_ref.trailer().cmp(upper_ref.trailer()))
    {
      cmp::Ordering::Greater | cmp::Ordering::Equal => None,
      _ => Some(Self {
        map,
        nd: map.head,
        lower: Some(lower),
        upper: Some(upper),
      }),
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
  /// if the iterator is pointing at a valid entry, and `None` otherwise.
  pub fn first(&mut self) -> Option<(KeyRef<K>, ValueRef<V>)> {
    unsafe {
      match &self.lower {
        Some(lower) => {
          self.nd = self.map.ge_in(lower.as_key_ref())?;
        }
        // No lower bound, so we can do quick path
        None => {
          self.nd = self.map.get_next(self.map.head, 0);
          if self.nd.ptr == self.map.tail.ptr {
            return None;
          }
        }
      }

      if let Some(ref upper) = self.upper {
        let upper = upper.as_key_ref();
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
      match &self.upper {
        Some(upper) => {
          self.nd = self.map.le_in(upper.as_key_ref())?;
        }
        // No upper bound, so we can do quick path
        None => {
          self.nd = self.map.get_prev(self.map.tail, 0);
          if self.nd.ptr == self.map.head.ptr {
            return None;
          }
        }
      }

      if let Some(ref lower) = self.lower {
        let lower = lower.as_key_ref();
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
        let upper = upper.as_key_ref();
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
        let lower = lower.as_key_ref();
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
  /// pointing at a valid entry, and `None` otherwise.
  pub fn seek_ge<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.nd = self.map.ge_in(key.as_key_ref())?;

    unsafe {
      // Safety: the nd is valid, we already check this
      let node = self.nd.as_ptr();
      // Safety: the node is allocated by the map's arena, so the key is valid
      let nk = node.get_key(&self.map.arena);

      if let Some(ref upper) = self.upper {
        let upper = upper.as_key_ref();
        if let cmp::Ordering::Less =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      if let Some(ref lower) = self.lower {
        let lower = lower.as_key_ref();
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

  /// Moves the iterator to the first entry whose key is greater than
  /// the given key. Returns the key and value if the iterator is
  /// pointing at a valid entry, and `None` otherwise.
  pub fn seek_gt<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.nd = self.map.gt_in(key.as_key_ref())?;

    unsafe {
      // Safety: the nd is valid, we already check this
      let node = self.nd.as_ptr();
      // Safety: the node is allocated by the map's arena, so the key is valid
      let nk = node.get_key(&self.map.arena);

      if let Some(ref upper) = self.upper {
        let upper = upper.as_key_ref();
        if let cmp::Ordering::Less =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      if let Some(ref lower) = self.lower {
        let lower = lower.as_key_ref();
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

  /// Moves the iterator to the first entry whose key is less than or
  /// equal to the given key. Returns the key and value if the iterator is
  /// pointing at a valid entry, and `None` otherwise.
  pub fn seek_le<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    // le_in has already checked the ptr is valid
    self.nd = self.map.le_in(key.as_key_ref())?;

    unsafe {
      // Safety: the nd is valid, we already check this on line 75
      let node = self.nd.as_ptr();
      // Safety: the node is allocated by the map's arena, so the key is valid
      let nk = node.get_key(&self.map.arena);

      if let Some(ref lower) = self.lower {
        let lower = lower.as_key_ref();
        if let cmp::Ordering::Greater = self.map.cmp.compare(lower.as_bytes(), nk) {
          self.nd = self.map.head;
          return None;
        } else if let cmp::Ordering::Greater = lower.trailer().cmp(&node.key_trailer) {
          self.nd = self.map.head;
          return None;
        }
      }

      if let Some(ref upper) = self.upper {
        let upper = upper.as_key_ref();
        if let cmp::Ordering::Less =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Less =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      // Safety: the nd is valid, we already check this on line 115
      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }

  /// Moves the iterator to the last entry whose key is less than the given
  /// key. Returns the key and value if the iterator is pointing at a valid entry,
  /// and `None` otherwise.
  pub fn seek_lt<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<(KeyRef<'a, K>, ValueRef<'a, V>)>
  where
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    // NB: the top-level MapIterator has already adjusted key based on
    // the upper-bound.
    self.nd = self.map.lt_in(key.as_key_ref())?;

    unsafe {
      // Safety: the nd is valid, we already check this on line 75
      let node = self.nd.as_ptr();
      // Safety: the node is allocated by the map's arena, so the key is valid
      let nk = node.get_key(&self.map.arena);

      if let Some(ref lower) = self.lower {
        let lower = lower.as_key_ref();
        if let cmp::Ordering::Greater | cmp::Ordering::Equal = self.map.cmp.compare(lower.as_bytes(), nk) {
          self.nd = self.map.head;
          return None;
        } else if let cmp::Ordering::Greater | cmp::Ordering::Equal = lower.trailer().cmp(&node.key_trailer) {
          self.nd = self.map.head;
          return None;
        }
      }

      if let Some(ref upper) = self.upper {
        let upper = upper.as_key_ref();
        if let cmp::Ordering::Equal | cmp::Ordering::Less =
          self.map.cmp.compare(upper.as_bytes(), nk)
        {
          self.nd = self.map.tail;
          return None;
        } else if let cmp::Ordering::Equal | cmp::Ordering::Less =
          upper.trailer().cmp(&node.key_trailer)
        {
          self.nd = self.map.tail;
          return None;
        }
      }

      // Safety: the nd is valid, we already check this on line 115
      Some((self.key_unchecked(), self.value_unchecked()))
    }
  }
}

impl<'a, K, V, L, U, C> MapIterator<'a, K, V, L, U, C>
where
  K: Key,
  V: Value,
  L: AsKeyRef<Key = K> + 'a,
  U: AsKeyRef<Key = K> + 'a,
  C: Comparator,
{
  /// Returns the key of the current entry without any checks.
  ///
  /// ## Safety
  /// - The current node must be valid
  unsafe fn key_unchecked(&self) -> KeyRef<'_, K> {
    let nd = self.nd.as_ptr();
    let val = nd.get_key(&self.map.arena);
    KeyRef::new(val, nd.key_trailer)
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
}
