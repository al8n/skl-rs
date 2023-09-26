use crate::{KeyRef, MapIterator};

use super::{AsKeyRef, Comparator, Key, SkipSet, VoidValue};

/// An iterator over the skipmap. The current state of the iterator can be cloned by
/// simply value copying the struct.
#[repr(transparent)]
pub struct SetIterator<'a, K, L, U, C = ()>
where
  K: Key,
  L: AsKeyRef<Key = K> + 'a,
  U: AsKeyRef<Key = K> + 'a,
  C: Comparator,
{
  iter: MapIterator<'a, K, VoidValue, L, U, C>,
}

impl<'a, K, C> SetIterator<'a, K, K, K, C>
where
  K: Key,
  C: Comparator,
{
  /// Creates a new iterator over the skipmap.
  #[inline]
  pub const fn new(set: &'a SkipSet<K, C>) -> Self {
    Self {
      iter: set.map.iter(),
    }
  }
}

impl<'a, K, L, C> SetIterator<'a, K, L, K, C>
where
  K: Key,
  L: AsKeyRef<Key = K> + 'a,
  C: Comparator,
{
  /// Creates a new iterator over the skipmap with the given lower bound.
  #[inline]
  pub const fn bound_lower(set: &'a SkipSet<K, C>, lower: L) -> Self {
    Self {
      iter: set.map.iter_bound_lower(lower),
    }
  }
}

impl<'a, K, U, C> SetIterator<'a, K, K, U, C>
where
  K: Key,
  U: AsKeyRef<Key = K> + 'a,
  C: Comparator,
{
  /// Creates a new iterator over the skipmap with the given upper bound.
  #[inline]
  pub const fn bound_upper(set: &'a SkipSet<K, C>, upper: U) -> Self {
    Self {
      iter: set.map.iter_bound_upper(upper),
    }
  }
}

impl<'a, K, L, U, C> SetIterator<'a, K, L, U, C>
where
  K: Key,
  L: AsKeyRef<Key = K> + 'a,
  U: AsKeyRef<Key = K> + 'a,
  C: Comparator,
{
  /// Creates a new iterator over the skipmap with the given lower and upper bounds.
  #[inline]
  pub fn bounded(set: &'a SkipSet<K, C>, lower: L, upper: U) -> Option<Self> {
    set.map.iter_bounded(lower, upper).map(|iter| Self { iter })
  }

  /// Seeks position at the first entry in map. Returns the key
  /// if the iterator is pointing at a valid entry, and `None` otherwise. Note
  /// that First only checks the upper bound. It is up to the caller to ensure
  /// that key is greater than or equal to the lower bound (e.g. via a call to `seek_ge(lower)`).
  pub fn first(&mut self) -> Option<KeyRef<K>> {
    self.iter.first().map(|ent| ent.0)
  }

  /// Seeks position at the last entry in list. Returns the key if
  /// the iterator is pointing at a valid entry, and `None` otherwise. Note
  /// that Last only checks the lower bound. It is up to the caller to ensure that
  /// key is less than the upper bound (e.g. via a call to `seek_lt(upper)`).
  pub fn last(&mut self) -> Option<KeyRef<K>> {
    self.iter.last().map(|ent| ent.0)
  }

  /// Advances to the next position. Returns the key if the
  /// iterator is pointing at a valid entry, and `None` otherwise.
  #[allow(clippy::should_implement_trait)]
  pub fn next(&mut self) -> Option<KeyRef<K>> {
    self.iter.next().map(|ent| ent.0)
  }

  /// Advances to the prev position. Returns the key if the
  /// iterator is pointing at a valid entry, and `None` otherwise.
  pub fn prev(&mut self) -> Option<KeyRef<K>> {
    self.iter.prev().map(|ent| ent.0)
  }

  /// Moves the iterator to the first entry whose key is greater than or
  /// equal to the given key. Returns the key if the iterator is
  /// pointing at a valid entry, and (nil, nil) otherwise. Note that `seek_ge` only
  /// checks the upper bound. It is up to the caller to ensure that key is greater
  /// than or equal to the lower bound.
  pub fn seek_ge<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<KeyRef<'a, K>>
  where
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.iter.seek_ge(key).map(|ent| ent.0)
  }

  /// Moves the iterator to the last entry whose key is less than the given
  /// key. Returns the key if the iterator is pointing at a valid entry,
  /// and `None` otherwise. Note that `seek_lt` only checks the lower bound. It
  /// is up to the caller to ensure that key is less than the upper bound.
  pub fn seek_lt<'k: 'a, Q>(&'a mut self, key: &'k Q) -> Option<KeyRef<'a, K>>
  where
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.iter.seek_lt(key).map(|ent| ent.0)
  }
}
