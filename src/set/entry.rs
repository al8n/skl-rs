use crate::{node::NodePtr, KeyRef};

use super::*;

/// An entry reference to the skipmap's entry.
pub struct EntryRef<'a, K: Key, C: Comparator> {
  pub(super) map: &'a SkipMap<K, VoidValue, C>,
  pub(super) nd: NodePtr<K::Trailer, ()>,
  pub(super) key: KeyRef<'a, K>,
}

impl<'a, K: Key, C: Comparator> EntryRef<'a, K, C> {
  /// Returns the reference to the key
  #[inline]
  pub const fn key(&self) -> &KeyRef<'a, K> {
    &self.key
  }

  /// Returns the prev entry linked to this entry if any, otherwise returns `None`.
  #[inline]
  pub fn prev(&self) -> Option<EntryRef<'a, K, C>> {
    unsafe {
      let prev_ptr = self.map.get_prev(self.nd, 0);

      if prev_ptr.ptr == self.map.head().ptr {
        return None;
      }
      let prev_node = prev_ptr.as_ptr();

      Some(Self {
        map: self.map,
        nd: prev_ptr,
        key: KeyRef::new(prev_node.get_key(self.map.arena()), prev_node.key_trailer),
      })
    }
  }

  /// Returns the next entry linked to this entry if any, otherwise returns `None`.
  #[inline]
  pub fn next(&self) -> Option<EntryRef<'a, K, C>> {
    unsafe {
      let next_ptr = self.map.get_next(self.nd, 0);

      if next_ptr.ptr == self.map.tail().ptr {
        return None;
      }
      let next_node = next_ptr.as_ptr();

      Some(Self {
        map: self.map,
        nd: next_ptr,
        key: KeyRef::new(next_node.get_key(self.map.arena()), next_node.key_trailer),
      })
    }
  }
}
