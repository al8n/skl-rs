use super::*;

/// An entry reference to the skipmap's entry.
pub struct EntryRef<'a, K: Key, V: Value, C: Comparator> {
  pub(super) map: &'a SkipMap<K, V, C>,
  pub(super) nd: NodePtr<K::Trailer, V::Trailer>,
  pub(super) key: KeyRef<'a, K>,
  pub(super) value: ValueRef<'a, V>,
}

impl<'a, K: Key, V: Value, C: Comparator> EntryRef<'a, K, V, C> {
  /// Returns the reference to the key
  #[inline]
  pub const fn key(&self) -> &KeyRef<'a, K> {
    &self.key
  }

  /// Returns the reference to the value
  #[inline]
  pub const fn value(&self) -> &ValueRef<'a, V> {
    &self.value
  }

  /// Returns the prev entry linked to this entry if any, otherwise returns `None`.
  #[inline]
  pub fn prev(&self) -> Option<EntryRef<'a, K, V, C>> {
    unsafe {
      let prev_ptr = self.map.get_prev(self.nd, 0);

      if prev_ptr.ptr == self.map.head.ptr {
        return None;
      }
      let prev_node = prev_ptr.as_ptr();

      Some(Self {
        map: self.map,
        nd: prev_ptr,
        key: KeyRef::new(prev_node.get_key(&self.map.arena), prev_node.key_trailer),
        value: ValueRef::new(
          prev_node.get_value(&self.map.arena),
          prev_node.value_trailer,
        ),
      })
    }
  }

  /// Returns the next entry linked to this entry if any, otherwise returns `None`.
  #[inline]
  pub fn next(&self) -> Option<EntryRef<'a, K, V, C>> {
    unsafe {
      let next_ptr = self.map.get_next(self.nd, 0);

      if next_ptr.ptr == self.map.tail.ptr {
        return None;
      }
      let next_node = next_ptr.as_ptr();

      Some(Self {
        map: self.map,
        nd: next_ptr,
        key: KeyRef::new(next_node.get_key(&self.map.arena), next_node.key_trailer),
        value: ValueRef::new(
          next_node.get_value(&self.map.arena),
          next_node.value_trailer,
        ),
      })
    }
  }

  #[inline]
  pub(crate) const fn ptr(&self) -> NodePtr<K::Trailer, V::Trailer> {
    self.nd
  }
}
