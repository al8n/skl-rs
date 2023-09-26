use core::cmp;

use crossbeam_utils::CachePadded;

use super::{
  arena::Arena,
  key::AsKeyRef,
  node::{Node, NodePtr},
  sync::{AtomicU32, Ordering},
  value::AsValueRef,
  Comparator, Key, KeyRef, Value, ValueRef, MAX_HEIGHT, NODE_ALIGNMENT_FACTOR,
};

mod error;
pub use error::Error;
mod entry;
pub use entry::*;
mod iterator;
pub use iterator::*;
mod snapshot;
pub use snapshot::*;

#[cfg(test)]
mod tests;

/// Precompute the skiplist probabilities so that only a single random number
/// needs to be generated and so that the optimal pvalue can be used (inverse
/// of Euler's number).
const PROBABILITIES: [u32; MAX_HEIGHT] = {
  const P: f64 = 1.0 / core::f64::consts::E;

  let mut probabilities = [0; MAX_HEIGHT];
  let mut p = 1f64;

  let mut i = 0;
  while i < MAX_HEIGHT {
    probabilities[i] = ((u32::MAX as f64) * p) as u32;
    p *= P;
    i += 1;
  }

  probabilities
};

/// A fast, cocnurrent map implementation based on skiplist that supports forward
/// and backward iteration. Keys and values are immutable once added to the skipmap and
/// deletion is not supported. Instead, higher-level code is expected to add new
/// entries that shadow existing entries and perform deletion via tombstones. It
/// is up to the user to process these shadow entries and tombstones
/// appropriately during retrieval.
#[derive(Debug)]
pub struct SkipMap<K: Key, V: Value, C: Comparator = ()> {
  arena: Arena,
  head: NodePtr<K::Trailer, V::Trailer>,
  tail: NodePtr<K::Trailer, V::Trailer>,

  /// Current height. 1 <= height <= kMaxHeight. CAS.
  height: CachePadded<AtomicU32>,
  len: CachePadded<AtomicU32>,

  /// If set to true by tests, then extra delays are added to make it easier to
  /// detect unusual race conditions.
  #[cfg(test)]
  testing: bool,

  cmp: C,
}

// Safety: SkipMap is Sync and Send
unsafe impl<K: Key, V: Value, C: Comparator> Send for SkipMap<K, V, C> {}
unsafe impl<K: Key, V: Value, C: Comparator> Sync for SkipMap<K, V, C> {}

// --------------------------------Public Methods--------------------------------
impl<K: Key, V: Value, C: Comparator> SkipMap<K, V, C> {
  /// Returns the height of the highest tower within any of the nodes that
  /// have ever been allocated as part of this skiplist.
  #[inline]
  pub fn height(&self) -> u32 {
    self.height.load(Ordering::Acquire)
  }

  /// Returns the arena backing this skipmap.
  #[inline]
  pub const fn arena(&self) -> &Arena {
    &self.arena
  }

  /// Returns the comparator used for the skipmap.
  #[inline]
  pub const fn cmp(&self) -> &C {
    &self.cmp
  }

  /// Returns the number of bytes that have allocated from the arena.
  #[inline]
  pub fn size(&self) -> usize {
    self.arena.size()
  }

  /// Returns the number of entries in the skipmap.
  #[inline]
  pub fn len(&self) -> usize {
    self.len.load(Ordering::Acquire) as usize
  }

  /// Returns true if the skipmap is empty.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

#[inline]
const fn alignment_assertion<K: Key, V: Value>() {
  assert!(((core::mem::align_of::<K::Trailer>() % NODE_ALIGNMENT_FACTOR) == 0) || (core::mem::size_of::<K::Trailer>() == 0), "Invalid Trailer type of key, the alignment of the types implement Trailer trait must be a multiple of 4 or (ZST) zero sized type.");
  assert!(((core::mem::align_of::<V::Trailer>() % NODE_ALIGNMENT_FACTOR) == 0) || (core::mem::size_of::<V::Trailer>() == 0), "Invalid Trailer type of value, the alignment of the types implement Trailer trait must be a multiple of 4 or (ZST) zero sized type.");
}

impl<K: Key, V: Value> SkipMap<K, V> {
  /// Create a new skipmap according to the given capacity
  ///
  /// **Note:** The capacity stands for how many memory allocated,
  /// it does not mean the skiplist can store `cap` entries.
  ///
  ///
  ///
  /// **What the difference between this method and [`SkipMap::mmap_anon`]?**
  ///
  /// 1. This method will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///   Even if we are working with raw pointers with `Box::into_raw`,
  ///   the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///   when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///   especially if you're frequently accessing or modifying it.
  ///
  /// 2. Where as [`SkipMap::mmap_anon`] will use mmap anonymous to require memory from the OS.
  ///   If you require very large contiguous memory regions, `mmap` might be more suitable because
  ///   it's more direct in requesting large chunks of memory from the OS.
  ///
  /// [`SkipMap::mmap_anon`]: #method.mmap_anon
  pub fn new(cap: usize) -> Result<Self, Error> {
    let arena = Arena::new_vec::<K, V>(cap);
    Self::new_in(arena, ())
  }

  /// Create a new skipmap according to the given capacity, and mmaped to a file.
  ///
  /// **Note:** The capacity stands for how many memory mmaped,
  /// it does not mean the skipmap can store `cap` entries.
  ///
  /// `lock`: whether to lock the underlying file or not
  #[cfg(feature = "mmap")]
  #[cfg_attr(docsrs, doc(cfg(feature = "mmap")))]
  pub fn mmap(cap: usize, file: std::fs::File, lock: bool) -> std::io::Result<Self> {
    let arena = Arena::new_mmap::<K, V>(cap, file, lock)?;
    Self::new_in(arena, ()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
  }

  /// Create a new skipmap according to the given capacity, and mmap anon.
  ///
  /// **What the difference between this method and [`SkipMap::new`]?**
  ///
  /// 1. This method will use mmap anonymous to require memory from the OS directly.
  ///   If you require very large contiguous memory regions, this method might be more suitable because
  ///   it's more direct in requesting large chunks of memory from the OS.
  ///
  /// 2. Where as [`SkipMap::new`] will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///   Even if we are working with raw pointers with `Box::into_raw`,
  ///   the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///   when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///   especially if you're frequently accessing or modifying it.
  ///
  /// [`SkipMap::new`]: #method.new
  #[cfg(feature = "mmap")]
  #[cfg_attr(docsrs, doc(cfg(feature = "mmap")))]
  pub fn mmap_anon(cap: usize) -> std::io::Result<Self> {
    let arena = Arena::new_anonymous_mmap::<K, V>(cap)?;
    Self::new_in(arena, ()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
  }
}

impl<K: Key, V: Value, C: Comparator> SkipMap<K, V, C> {
  /// Set comparator for the skipmap.
  pub fn with_comparator<NC: Comparator>(this: SkipMap<K, V, NC>, cmp: C) -> Self {
    Self {
      arena: this.arena,
      head: this.head,
      tail: this.tail,
      height: this.height,
      #[cfg(test)]
      testing: this.testing,
      cmp,
      len: this.len,
    }
  }

  /// Returns true if the key exists in the map.
  #[inline]
  pub fn contains_key<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> bool
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
  {
    self.get(key).is_some()
  }

  /// Returns the first entry in the map.
  pub fn first(&self) -> Option<EntryRef<K, V, C>> {
    // Safety: head node was definitely allocated by self.arena
    let nd = unsafe { self.get_next(self.head, 0) };
    if nd.is_null() || nd.ptr == self.tail.ptr {
      return None;
    }

    // Safety: we already check the not is not null, and the node is allocated by self.arena
    unsafe {
      let node = nd.as_ptr();
      Some(EntryRef {
        map: self,
        nd,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      })
    }
  }

  /// Returns the last entry in the map.
  pub fn last(&self) -> Option<EntryRef<K, V, C>> {
    // Safety: tail node was definitely allocated by self.arena
    let nd = unsafe { self.get_prev(self.tail, 0) };
    if nd.is_null() || nd.ptr == self.head.ptr {
      return None;
    }

    // Safety: we already check the not is not null, and the node is allocated by self.arena
    unsafe {
      let node = nd.as_ptr();
      Some(EntryRef {
        map: self,
        nd,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      })
    }
  }

  /// Returns the `i`-th entry in the skip map.
  /// This operation is `O(2/n)`.
  pub fn ith(&self, index: usize) -> Option<EntryRef<K, V, C>> {
    let len = self.len();
    if index >= len {
      return None;
    }

    unsafe {
      // located in the first half
      let nd = if index < len / 2 {
        let mut nd = self.head;
        for _ in 0..index {
          nd = self.get_next(nd, 0);
        }
        nd
      } else {
        let mut nd = self.tail;
        for _ in 0..index {
          nd = self.get_prev(nd, 0);
        }
        nd
      };

      let node = nd.as_ptr();
      Some(EntryRef {
        map: self,
        nd,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      })
    }
  }

  /// Returns the value associated with the given key, if it exists.
  pub fn get<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<ValueRef<'a, V>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    let key = key.as_key_ref();
    self.get_in(key).map(|ptr| unsafe {
      let node = ptr.as_ptr();
      ValueRef::new(node.get_value(&self.arena), node.value_trailer)
    })
  }

  /// Returns the entry greater or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, key is equal to k1, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  pub fn gt<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, V, C>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.gt_in(key.as_key_ref()).map(|ptr| unsafe {
      // Safety: the gt_in guarantees that ptr is valid,
      let node = ptr.as_ptr();
      EntryRef {
        map: self,
        nd: ptr,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      }
    })
  }

  /// Returns the entry less than the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  pub fn lt<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, V, C>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.lt_in(key.as_key_ref()).map(|ptr| unsafe {
      // Safety: the lt_in guarantees that ptr is valid,
      let node = ptr.as_ptr();
      EntryRef {
        map: self,
        nd: ptr,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      }
    })
  }

  /// Returns the entry greater than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, key is equal to k1, then the entry contains k1 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  pub fn ge<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, V, C>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.ge_in(key.as_key_ref()).map(|ptr| unsafe {
      // Safety: the ge_in guarantees that ptr is valid,
      let node = ptr.as_ptr();
      EntryRef {
        map: self,
        nd: ptr,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      }
    })
  }

  /// Returns the entry less than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k3 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  pub fn le<'a, 'b: 'a, Q>(&'a self, key: &'b Q) -> Option<EntryRef<'a, K, V, C>>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
  {
    self.le_in(key.as_key_ref()).map(|ptr| unsafe {
      // Safety: the le_in guarantees that ptr is valid,
      let node = ptr.as_ptr();
      EntryRef {
        map: self,
        nd: ptr,
        key: KeyRef::new(node.get_key(&self.arena), node.key_trailer),
        value: ValueRef::new(node.get_value(&self.arena), node.value_trailer),
      }
    })
  }

  /// Gets or inserts a new entry.
  ///
  /// # Success
  ///
  /// - Returns `Ok(Some(&[u8]))` if the key exists.
  /// - Returns `Ok(None)` if the key does not exist, and successfully inserts the key and value.
  ///
  /// As a low-level crate, users are expected to handle the error cases themselves.
  ///
  /// # Errors
  ///
  /// - Returns `Err(Error::Duplicated)`, if the key already exists.
  /// - Returns `Err(Error::Full)`, if there isn't enough room in the arena.
  pub fn get_or_insert<'a, 'b: 'a, Q, R>(
    &'a self,
    key: &'b Q,
    value: &'b R,
  ) -> Result<Option<ValueRef<'a, V>>, Error>
  where
    K::Trailer: 'a,
    V::Trailer: 'a,
    Q: ?Sized + AsKeyRef<Key = K>,
    R: AsValueRef<Value = V> + ?Sized,
  {
    let key = key.as_key_ref();
    let val = value.as_value_ref();
    let ins = &mut Default::default();

    unsafe {
      let (_, curr) = self.find_splice(key, ins, true);
      if let Some(curr) = curr {
        return Ok(Some({
          let nd = curr.as_ptr();
          ValueRef::new(nd.get_value(&self.arena), nd.value_trailer)
        }));
      }
      self.insert_in(key, val, ins).map(|_| None)
    }
  }

  /// Inserts a new key if it does not yet exist. Returns `Ok(())` if the key was successfully inserted.
  ///
  /// As a low-level crate, users are expected to handle the error cases themselves.
  ///
  /// # Errors
  ///
  /// - Returns `Error::Duplicated`, if the key already exists.
  /// - Returns `Error::Full`, if there isn't enough room in the arena.
  pub fn insert<'a, 'b: 'a, Q, R>(&'a self, key: &'b Q, value: &'b R) -> Result<(), Error>
  where
    K::Trailer: 'b,
    V::Trailer: 'b,
    Q: Ord + ?Sized + AsKeyRef<Key = K>,
    R: AsValueRef<Value = V> + ?Sized,
  {
    self.insert_in(
      key.as_key_ref(),
      value.as_value_ref(),
      &mut Inserter::default(),
    )
  }

  /// Returns a new `Iterator`. Note that it is
  /// safe for an iterator to be copied by value.
  #[inline]
  pub const fn iter(&self) -> iterator::MapIterator<K, V, K, K, C> {
    iterator::MapIterator::new(self)
  }

  /// Returns a new `Iterator` that with the lower and upper bounds.
  ///
  /// # Returns
  /// - `None`: the upper >= lower || lower is larger than the tail || upper is smaller than the head
  /// - `Some(..)`: both lower and upper bounds are valid
  #[inline]
  pub fn iter_bounded<'a, L, U>(
    &'a self,
    lower: L,
    upper: U,
  ) -> Option<iterator::MapIterator<'a, K, V, L, U, C>>
  where
    L: AsKeyRef<Key = K> + 'a,
    U: AsKeyRef<Key = K> + 'a,
  {
    iterator::MapIterator::bounded(self, lower, upper)
  }

  /// Returns a new `Iterator` that with the lower bound.
  #[inline]
  pub const fn iter_bound_lower<'a, L>(
    &'a self,
    lower: L,
  ) -> iterator::MapIterator<'a, K, V, L, K, C>
  where
    L: AsKeyRef<Key = K> + 'a,
  {
    iterator::MapIterator::bound_lower(self, lower)
  }

  /// Returns a new `Iterator` that with the upper bound.
  #[inline]
  pub const fn iter_bound_upper<'a, U>(
    &'a self,
    upper: U,
  ) -> iterator::MapIterator<'a, K, V, K, U, C>
  where
    U: AsKeyRef<Key = K> + 'a,
  {
    iterator::MapIterator::bound_upper(self, upper)
  }

  /// Returns an optional snapshot of the SkipMap's current state.
  ///
  /// A snapshot is a read-only view of the SkipMap at a specific point in time.
  /// It provides a consistent view of the SkipMap's contents, which can be useful
  /// for various purposes, such as implementing transactional semantics.
  ///
  /// # Returns
  ///
  /// - `Some(snapshot::Snapshot)`: If the SkipMap's is not empty.
  /// - `None`: If the current SkipMap is empty.
  ///
  #[inline]
  pub fn snapshot(&self) -> Option<snapshot::Snapshot<K, V, C>> {
    let height = self.height();
    let len = self.len() as u32;
    let last = self.ith(len as usize - 1)?.nd;
    let map = Self {
      arena: self.arena.clone(),
      head: self.head,
      tail: self.tail,
      height: CachePadded::new(AtomicU32::new(height)),
      len: CachePadded::new(AtomicU32::new(len)),
      #[cfg(test)]
      testing: self.testing,
      cmp: self.cmp.clone(),
    };

    Some(snapshot::Snapshot::new(
      map,
      height,
      len,
      self.arena.size(),
      last,
    ))
  }
}

// --------------------------------Crate Level Methods--------------------------------
impl<K: Key, V: Value, C: Comparator> SkipMap<K, V, C> {
  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  pub(crate) unsafe fn get_prev(
    &self,
    nd: NodePtr<K::Trailer, V::Trailer>,
    height: usize,
  ) -> NodePtr<K::Trailer, V::Trailer> {
    if nd.is_null() {
      return NodePtr::NULL;
    }

    let offset = nd.prev_offset(&self.arena, height);
    let ptr = self.arena.get_pointer(offset as usize);
    NodePtr::new(ptr, offset)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  pub(crate) unsafe fn get_next(
    &self,
    nptr: NodePtr<K::Trailer, V::Trailer>,
    height: usize,
  ) -> NodePtr<K::Trailer, V::Trailer> {
    if nptr.is_null() {
      return NodePtr::NULL;
    }
    let offset = nptr.next_offset(&self.arena, height);
    let ptr = self.arena.get_pointer(offset as usize);
    NodePtr::new(ptr, offset)
  }

  #[inline]
  pub(crate) const fn head(&self) -> NodePtr<K::Trailer, V::Trailer> {
    self.head
  }

  #[inline]
  pub(crate) const fn tail(&self) -> NodePtr<K::Trailer, V::Trailer> {
    self.tail
  }
}

// --------------------------------Private Methods--------------------------------
impl<K: Key, V: Value, C: Comparator> SkipMap<K, V, C> {
  fn new_in(arena: Arena, cmp: C) -> Result<Self, Error> {
    alignment_assertion::<K, V>();
    let head = Node::<K::Trailer, V::Trailer>::new_empty_node_ptr(&arena)?;
    let tail = Node::<K::Trailer, V::Trailer>::new_empty_node_ptr(&arena)?;

    // Safety:
    // We will always allocate enough space for the head node and the tail node.
    unsafe {
      // Link all head/tail levels together.
      for i in 0..MAX_HEIGHT {
        let head_link = head.tower(&arena, i);
        let tail_link = tail.tower(&arena, i);
        head_link.next_offset.store(tail.offset, Ordering::Relaxed);
        tail_link.prev_offset.store(head.offset, Ordering::Relaxed);
      }
    }

    Ok(Self {
      arena,
      head,
      tail,
      height: CachePadded::new(AtomicU32::new(1)),
      #[cfg(test)]
      testing: false,
      cmp,
      len: CachePadded::new(AtomicU32::new(0)),
    })
  }

  #[allow(clippy::type_complexity)]
  fn new_node(
    &self,
    key: &KeyRef<K>,
    value: &ValueRef<V>,
  ) -> Result<(NodePtr<K::Trailer, V::Trailer>, u32), Error> {
    let height = Self::random_height();
    let nd = Node::new_node_ptr(&self.arena, height, key, value)?;

    // Try to increase self.height via CAS.
    let mut list_height = self.height();
    while height > list_height {
      match self.height.compare_exchange_weak(
        list_height,
        height,
        Ordering::SeqCst,
        Ordering::Acquire,
      ) {
        // Successfully increased skiplist.height.
        Ok(_) => break,
        Err(h) => list_height = h,
      }
    }
    Ok((nd, height))
  }

  #[cfg(feature = "std")]
  #[inline]
  fn random_height() -> u32 {
    use rand::{thread_rng, Rng};
    let mut rng = thread_rng();
    let rnd: u32 = rng.gen();
    let mut h = 1;

    while h < MAX_HEIGHT && rnd <= PROBABILITIES[h] {
      h += 1;
    }
    h as u32
  }

  #[cfg(not(feature = "std"))]
  #[inline]
  fn random_height() -> u32 {
    use rand::{rngs::OsRng, Rng, RngCore};

    let rnd: u32 = OsRng.gen();
    let mut h = 1;

    while h < MAX_HEIGHT && rnd <= PROBABILITIES[h] {
      h += 1;
    }
    h as u32
  }

  fn get_in(&self, key: KeyRef<K>) -> Option<NodePtr<K::Trailer, V::Trailer>> {
    let mut lvl = (self.height() - 1) as usize;

    let mut prev = self.head;
    loop {
      let fr = unsafe { self.find_splice_for_level(&key, lvl, prev) };
      if fr.found {
        return fr.curr;
      }
      if lvl == 0 {
        break;
      }

      prev = fr.splice.prev;
      lvl -= 1;
    }

    None
  }

  fn lt_in(&self, key: KeyRef<K>) -> Option<NodePtr<K::Trailer, V::Trailer>> {
    let res = self.seek_for_base_splice(key);
    let nd = res.prev;
    if nd.ptr == self.head.ptr {
      return None;
    }
    Some(nd)
  }

  fn gt_in(&self, key: KeyRef<K>) -> Option<NodePtr<K::Trailer, V::Trailer>> {
    let res = self.seek_for_base_splice(key);
    let mut nd = res.next;
    if nd.ptr == self.tail.ptr {
      return None;
    }

    unsafe {
      let node = nd.as_ptr();
      if self.cmp.compare(key.as_bytes(), node.get_key(&self.arena)) == cmp::Ordering::Equal
        && node.key_trailer.eq(key.trailer())
      {
        nd = self.get_next(nd, 0);
        if nd.ptr == self.tail.ptr {
          return None;
        }
      }
    }
    Some(nd)
  }

  fn le_in(&self, key: KeyRef<K>) -> Option<NodePtr<K::Trailer, V::Trailer>> {
    let res = self.seek_for_base_splice(key);
    let mut nd = res.prev;

    unsafe {
      let node = res.next.as_ptr();

      if self.cmp.compare(key.as_bytes(), node.get_key(&self.arena)) == cmp::Ordering::Equal
        && node.key_trailer.eq(key.trailer())
      {
        nd = res.next;
      }

      if nd.ptr == self.head.ptr {
        return None;
      }
    }
    Some(nd)
  }

  fn ge_in(&self, key: KeyRef<K>) -> Option<NodePtr<K::Trailer, V::Trailer>> {
    let res = self.seek_for_base_splice(key);
    let nd = res.next;
    if nd.ptr == self.tail.ptr {
      return None;
    }

    Some(nd)
  }

  fn insert_in(
    &self,
    key: KeyRef<K>,
    value: ValueRef<V>,
    ins: &mut Inserter<K, V>,
  ) -> Result<(), Error> {
    // Safety: a fresh new Inserter, so safe here
    if unsafe { self.find_splice(key, ins, false).0 } {
      return Err(Error::Duplicated);
    }

    #[cfg(all(test, feature = "std"))]
    if self.testing {
      // Add delay to make it easier to test race between this thread
      // and another thread that sees the intermediate state between
      // finding the splice and using it.
      std::thread::yield_now();
    }

    let (nd, height) = self.new_node(&key, &value)?;
    // We always insert from the base level and up. After you add a node in base
    // level, we cannot create a node in the level above because it would have
    // discovered the node in the base level.
    let mut invalid_date_splice = false;

    for i in 0..(height as usize) {
      let mut prev = ins.spl[i].prev;
      let mut next = ins.spl[i].next;

      if prev.is_null() {
        // New node increased the height of the skiplist, so assume that the
        // new level has not yet been populated.
        if !next.is_null() {
          panic!("next is expected to be nil, since prev is nil");
        }

        prev = self.head;
        next = self.tail;
      }

      // +----------------+     +------------+     +----------------+
      // |      prev      |     |     nd     |     |      next      |
      // | prevNextOffset |---->|            |     |                |
      // |                |<----| prevOffset |     |                |
      // |                |     | nextOffset |---->|                |
      // |                |     |            |<----| nextPrevOffset |
      // +----------------+     +------------+     +----------------+
      //
      // 1. Initialize prevOffset and nextOffset to point to prev and next.
      // 2. CAS prevNextOffset to repoint from next to nd.
      // 3. CAS nextPrevOffset to repoint from prev to nd.
      unsafe {
        loop {
          let prev_offset = prev.offset;
          let next_offset = next.offset;
          nd.write_tower(&self.arena, i, prev_offset, next_offset);

          // Check whether next has an updated link to prev. If it does not,
          // that can mean one of two things:
          //   1. The thread that added the next node hasn't yet had a chance
          //      to add the prev link (but will shortly).
          //   2. Another thread has added a new node between prev and next.
          //
          // Safety: we already check next is not null
          let next_prev_offset = next.prev_offset(&self.arena, i);
          if next_prev_offset != prev_offset {
            // Determine whether #1 or #2 is true by checking whether prev
            // is still pointing to next. As long as the atomic operations
            // have at least acquire/release semantics (no need for
            // sequential consistency), this works, as it is equivalent to
            // the "publication safety" pattern.
            let prev_next_offset = prev.next_offset(&self.arena, i);
            if prev_next_offset == next_offset {
              // Ok, case #1 is true, so help the other thread along by
              // updating the next node's prev link.
              let link = next.tower(&self.arena, i);
              let _ = link.prev_offset.compare_exchange(
                next_prev_offset,
                prev_offset,
                Ordering::SeqCst,
                Ordering::Acquire,
              );
            }
          }

          let prev_link = prev.tower(&self.arena, i);
          match prev_link.next_offset.compare_exchange_weak(
            next.offset,
            nd.offset,
            Ordering::SeqCst,
            Ordering::Acquire,
          ) {
            Ok(_) => {
              // Managed to insert nd between prev and next, so update the next
              // node's prev link and go to the next level.
              #[cfg(all(test, feature = "std"))]
              if self.testing {
                // Add delay to make it easier to test race between this thread
                // and another thread that sees the intermediate state between
                // setting next and setting prev.
                std::thread::yield_now();
              }

              let next_link = next.tower(&self.arena, i);
              let _ = next_link.prev_offset.compare_exchange(
                prev_offset,
                nd.offset,
                Ordering::SeqCst,
                Ordering::Acquire,
              );

              break;
            }
            Err(_) => {
              // CAS failed. We need to recompute prev and next. It is unlikely to
              // be helpful to try to use a different level as we redo the search,
              // because it is unlikely that lots of nodes are inserted between prev
              // and next.
              let fr = self.find_splice_for_level(&key, i, prev);
              if fr.found {
                if i != 0 {
                  panic!("how can another thread have inserted a node at a non-base level?");
                }

                return Err(Error::Duplicated);
              }

              invalid_date_splice = true;
              prev = fr.splice.prev;
              next = fr.splice.next;
            }
          }
        }
      }
    }

    // If we had to recompute the splice for a level, invalidate the entire
    // cached splice.
    if invalid_date_splice {
      ins.height = 0;
    } else {
      // The splice was valid. We inserted a node between spl[i].prev and
      // spl[i].next. Optimistically update spl[i].prev for use in a subsequent
      // call to add.
      for i in 0..(height as usize) {
        ins.spl[i].prev = nd;
      }
    }
    self.len.fetch_add(1, Ordering::AcqRel);
    Ok(())
  }

  /// ## Safety:
  /// - All of splices in the inserter must be contains node ptrs are allocated by the current skip map.
  unsafe fn find_splice(
    &self,
    key: KeyRef<K>,
    ins: &mut Inserter<K, V>,
    returned_when_found: bool,
  ) -> (bool, Option<NodePtr<K::Trailer, V::Trailer>>) {
    let list_height = self.height();
    let mut level = 0;

    let mut prev = self.head;
    if ins.height < list_height {
      // Our cached height is less than the list height, which means there were
      // inserts that increased the height of the list. Recompute the splice from
      // scratch.
      ins.height = list_height;
      level = ins.height as usize;
    } else {
      // Our cached height is equal to the list height.
      while level < list_height as usize {
        let spl = &ins.spl[level];
        if self.get_next(spl.prev, level).ptr != spl.next.ptr {
          level += 1;
          // One or more nodes have been inserted between the splice at this
          // level.
          continue;
        }

        if spl.prev.ptr != self.head.ptr && !self.key_is_after_node(spl.prev, key) {
          // Key lies before splice.
          level = list_height as usize;
          break;
        }

        if spl.next.ptr != self.tail.ptr && !self.key_is_after_node(spl.next, key) {
          // Key lies after splice.
          level = list_height as usize;
          break;
        }

        // The splice brackets the key!
        prev = spl.prev;
        break;
      }
    }

    let mut found = false;
    for lvl in (0..level).rev() {
      let mut fr = self.find_splice_for_level(&key, lvl, prev);
      if fr.splice.next.is_null() {
        fr.splice.next = self.tail;
      }
      found = fr.found;
      if found && returned_when_found {
        return (found, fr.curr);
      }
      ins.spl[lvl] = fr.splice;
    }

    (found, None)
  }

  /// ## Safety
  /// - `level` is less than `MAX_HEIGHT`.
  /// - `start` must be allocated by self's arena.
  unsafe fn find_splice_for_level(
    &self,
    key: &KeyRef<K>,
    level: usize,
    start: NodePtr<K::Trailer, V::Trailer>,
  ) -> FindResult<K, V> {
    let mut prev = start;

    loop {
      // Assume prev.key < key.
      let next = self.get_next(prev, level);
      if next.ptr == self.tail.ptr {
        // Tail node, so done.
        return FindResult {
          splice: Splice { prev, next },
          found: false,
          curr: None,
        };
      }

      // offset is not zero, so we can safely dereference the next node ptr.
      let next_node = next.as_ptr();

      let (key_offset, key_size) = (next_node.key_offset, next_node.key_size);
      let next_key = self.arena.get_bytes(key_offset as usize, key_size as usize);

      match self.cmp.compare(key.as_bytes(), next_key) {
        // We are done for this level, since prev.key < key < next.key.
        cmp::Ordering::Less => {
          return FindResult {
            splice: Splice { prev, next },
            found: false,
            curr: None,
          };
        }
        // Keep moving right on this level.
        cmp::Ordering::Greater => prev = next,
        cmp::Ordering::Equal => {
          // User-key equality.
          let trailer = key.trailer();

          if trailer.eq(&next_node.key_trailer) {
            // Internal key equality.
            return FindResult {
              splice: Splice { prev, next },
              found: true,
              curr: Some(next),
            };
          }

          if trailer.gt(&next_node.key_trailer) {
            // We are done for this level, since prev.key < key < next.key.
            return FindResult {
              splice: Splice { prev, next },
              found: false,
              curr: None,
            };
          }

          // Keep moving right on this level.
          prev = next;
        }
      }
    }
  }

  /// ## Safety
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the node is not null.
  unsafe fn key_is_after_node(&self, nd: NodePtr<K::Trailer, V::Trailer>, key: KeyRef<K>) -> bool {
    let nd = &*nd.ptr;
    let nd_key = self
      .arena
      .get_bytes(nd.key_offset as usize, nd.key_size as usize);

    match self.cmp.compare(nd_key, key.as_bytes()) {
      cmp::Ordering::Less => true,
      cmp::Ordering::Greater => false,
      cmp::Ordering::Equal => {
        // User-key equality.
        let key_trailer = key.trailer();
        if nd.key_trailer.eq(key_trailer) {
          // Trailer equality.
          return false;
        }
        nd.key_trailer.le(key_trailer)
      }
    }
  }

  fn seek_for_base_splice(&self, key: KeyRef<K>) -> SeekResult<K, V> {
    let mut lvl = (self.height() - 1) as usize;

    let mut prev = self.head;
    let mut next;

    loop {
      let fr = unsafe { self.find_splice_for_level(&key, lvl, prev) };
      prev = fr.splice.prev;
      next = fr.splice.next;
      if fr.found {
        if lvl != 0 {
          // next is pointing at the target node, but we need to find previous on
          // the bottom level.

          // Safety: the next we use here is got from the find_splice_for_level, so must be allocated by the same arena
          prev = unsafe { self.get_prev(next, 0) };
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

/// A helper struct for caching splice information
pub struct Inserter<'a, K: Key, V: Value> {
  spl: [Splice<K, V>; MAX_HEIGHT],
  height: u32,
  _m: core::marker::PhantomData<&'a ()>,
}

impl<'a, K: Key, V: Value> Default for Inserter<'a, K, V> {
  #[inline]
  fn default() -> Self {
    Self {
      spl: [Splice::default(); MAX_HEIGHT],
      height: 0,
      _m: core::marker::PhantomData,
    }
  }
}

struct Splice<K: Key, V: Value> {
  prev: NodePtr<K::Trailer, V::Trailer>,
  next: NodePtr<K::Trailer, V::Trailer>,
}

impl<K: Key, V: Value> Default for Splice<K, V> {
  #[inline]
  fn default() -> Self {
    Self {
      prev: NodePtr::NULL,
      next: NodePtr::NULL,
    }
  }
}

impl<K: Key, V: Value> Clone for Splice<K, V> {
  #[inline]
  fn clone(&self) -> Self {
    *self
  }
}

impl<K: Key, V: Value> Copy for Splice<K, V> {}

struct FindResult<K: Key, V: Value> {
  found: bool,
  splice: Splice<K, V>,
  curr: Option<NodePtr<K::Trailer, V::Trailer>>,
}

struct SeekResult<K: Key, V: Value> {
  prev: NodePtr<K::Trailer, V::Trailer>,
  next: NodePtr<K::Trailer, V::Trailer>,
}
