use core::{
  cmp,
  convert::Infallible,
  marker::PhantomData,
  mem,
  ops::{Bound, RangeBounds},
  ptr::{self, NonNull},
};

use std::boxed::Box;

use crate::{Key, Trailer, VacantBuffer};

#[cfg(all(feature = "memmap", not(target_family = "wasm")))]
use error::{bad_magic_version, bad_version, invalid_data};

use super::{sync::*, Arena, Ascend, Comparator, *};

mod api;

use either::Either;

mod error;
pub use error::Error;
mod entry;
pub use entry::*;
mod iterator;
pub use iterator::*;

use rarena_allocator::Error as ArenaError;

#[cfg(test)]
mod tests;

const CURRENT_VERSION: u16 = 0;

/// The tombstone value size, if a node's value size is equal to this value, then it is a tombstone.
const REMOVE: u32 = u32::MAX;

type UpdateOk<'a, 'b, T, C> = Either<
  Option<VersionedEntryRef<'a, T, C>>,
  Result<VersionedEntryRef<'a, T, C>, VersionedEntryRef<'a, T, C>>,
>;

#[derive(Debug)]
#[repr(C)]
struct Meta {
  /// The maximum MVCC version of the skiplist. CAS.
  max_version: AtomicU64,
  /// The minimum MVCC version of the skiplist. CAS.
  min_version: AtomicU64,
  len: AtomicU32,
  magic_version: u16,
  /// Current height. 1 <= height <= 31. CAS.
  height: AtomicU8,
  reserved_byte: u8,
}

impl Meta {
  #[inline]
  fn new(version: u16) -> Self {
    Self {
      max_version: AtomicU64::new(0),
      min_version: AtomicU64::new(0),
      magic_version: version,
      height: AtomicU8::new(1),
      len: AtomicU32::new(0),
      reserved_byte: 0,
    }
  }

  #[inline]
  const fn magic_version(&self) -> u16 {
    self.magic_version
  }

  #[inline]
  fn max_version(&self) -> u64 {
    self.max_version.load(Ordering::Acquire)
  }

  #[inline]
  fn min_version(&self) -> u64 {
    self.min_version.load(Ordering::Acquire)
  }

  #[inline]
  fn height(&self) -> u8 {
    self.height.load(Ordering::Acquire)
  }

  #[inline]
  fn len(&self) -> u32 {
    self.len.load(Ordering::Acquire)
  }

  #[inline]
  fn increase_len(&self) {
    self.len.fetch_add(1, Ordering::Release);
  }

  fn update_max_version(&self, version: u64) {
    let mut current = self.max_version.load(Ordering::Acquire);

    loop {
      if version <= current {
        return;
      }

      match self.max_version.compare_exchange_weak(
        current,
        version,
        Ordering::SeqCst,
        Ordering::Acquire,
      ) {
        Ok(_) => break,
        Err(v) => current = v,
      }
    }
  }

  fn update_min_version(&self, version: u64) {
    let mut current = self.min_version.load(Ordering::Acquire);

    loop {
      if version >= current {
        return;
      }

      match self.min_version.compare_exchange_weak(
        current,
        version,
        Ordering::SeqCst,
        Ordering::Acquire,
      ) {
        Ok(_) => break,
        Err(v) => current = v,
      }
    }
  }
}

#[repr(C, align(8))]
pub(crate) struct AtomicValuePointer(AtomicU64);

impl core::fmt::Debug for AtomicValuePointer {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let (offset, len) = decode_value_pointer(self.0.load(Ordering::Relaxed));
    f.debug_struct("AtomicValuePointer")
      .field("offset", &offset)
      .field("len", &len)
      .finish()
  }
}

impl AtomicValuePointer {
  #[inline]
  fn new(offset: u32, len: u32) -> Self {
    Self(AtomicU64::new(encode_value_pointer(offset, len)))
  }

  #[inline]
  fn load(&self, ordering: Ordering) -> (u32, u32) {
    decode_value_pointer(self.0.load(ordering))
  }

  #[inline]
  fn swap(&self, offset: u32, len: u32) -> (u32, u32) {
    decode_value_pointer(
      self
        .0
        .swap(encode_value_pointer(offset, len), Ordering::AcqRel),
    )
  }

  #[inline]
  fn compare_remove(&self, success: Ordering, failure: Ordering) -> Result<(u32, u32), (u32, u32)> {
    let old = self.0.load(Ordering::Acquire);
    let (offset, _) = decode_value_pointer(old);
    let new = encode_value_pointer(offset, REMOVE);
    self
      .0
      .compare_exchange(old, new, success, failure)
      .map(decode_value_pointer)
      .map_err(decode_value_pointer)
  }
}

#[derive(Debug)]
struct NodePtr<T> {
  ptr: *mut Node<T>,
  offset: u32,
}

impl<T> Clone for NodePtr<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<T> Copy for NodePtr<T> {}

impl<T> NodePtr<T> {
  const NULL: Self = Self {
    ptr: ptr::null_mut(),
    offset: 0,
  };

  #[inline]
  const fn new(ptr: *mut u8, offset: u32) -> Self {
    Self {
      ptr: ptr.cast(),
      offset,
    }
  }

  #[inline]
  const fn is_null(&self) -> bool {
    self.offset == 0
  }

  /// ## Safety
  /// - the pointer must be valid
  #[inline]
  unsafe fn as_ref(&self) -> &Node<T> {
    &*self.ptr.cast()
  }

  /// ## Safety
  /// - the pointer must be valid
  #[inline]
  unsafe fn as_mut(&self) -> &mut Node<T> {
    &mut *self.ptr.cast()
  }

  #[inline]
  unsafe fn tower(&self, arena: &Arena, idx: usize) -> &Link {
    let tower_ptr_offset = self.offset as usize + Node::<T>::SIZE + idx * Link::SIZE;
    let tower_ptr = arena.get_pointer(tower_ptr_offset);
    &*tower_ptr.cast()
  }

  #[inline]
  unsafe fn write_tower(&self, arena: &Arena, idx: usize, prev_offset: u32, next_offset: u32) {
    let tower_ptr_offset = self.offset as usize + Node::<T>::SIZE + idx * Link::SIZE;
    let tower_ptr: *mut Link = arena.get_pointer_mut(tower_ptr_offset).cast();
    *tower_ptr = Link::new(next_offset, prev_offset);
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the offset is less than the capacity of the arena and larger than 0.
  unsafe fn next_offset(&self, arena: &Arena, idx: usize) -> u32 {
    self.tower(arena, idx).next_offset.load(Ordering::Acquire)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the offset is less than the capacity of the arena and larger than 0.
  unsafe fn prev_offset(&self, arena: &Arena, idx: usize) -> u32 {
    self.tower(arena, idx).prev_offset.load(Ordering::Acquire)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the offset is less than the capacity of the arena and larger than 0.
  unsafe fn cas_prev_offset(
    &self,
    arena: &Arena,
    idx: usize,
    current: u32,
    new: u32,
    success: Ordering,
    failure: Ordering,
  ) -> Result<u32, u32> {
    #[cfg(not(feature = "unaligned"))]
    self
      .tower(arena, idx)
      .prev_offset
      .compare_exchange(current, new, success, failure)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the offset is less than the capacity of the arena and larger than 0.
  unsafe fn cas_next_offset(
    &self,
    arena: &Arena,
    idx: usize,
    current: u32,
    new: u32,
    success: Ordering,
    failure: Ordering,
  ) -> Result<u32, u32> {
    self
      .tower(arena, idx)
      .next_offset
      .compare_exchange(current, new, success, failure)
  }
}

#[derive(Debug)]
#[repr(C)]
struct Link {
  next_offset: AtomicU32,
  prev_offset: AtomicU32,
}

impl Link {
  const SIZE: usize = mem::size_of::<Self>();

  #[inline]
  fn new(next_offset: u32, prev_offset: u32) -> Self {
    Self {
      next_offset: AtomicU32::new(next_offset),
      prev_offset: AtomicU32::new(prev_offset),
    }
  }
}

#[repr(C)]
struct Node<T> {
  // A byte slice is 24 bytes. We are trying to save space here.
  /// Multiple parts of the value are encoded as a single uint64 so that it
  /// can be atomically loaded and stored:
  ///   value offset: u32 (bits 0-31)
  ///   value size  : u32 (bits 32-63)
  value: AtomicValuePointer,
  // Immutable. No need to lock to access key.
  key_offset: u32,
  // Immutable. No need to lock to access key.
  key_size_and_height: u32,
  trailer: PhantomData<T>,
  // ** DO NOT REMOVE BELOW COMMENT**
  // The below field will be attached after the node, have to comment out
  // this field, because each node will not use the full height, the code will
  // not allocate the full size of the tower.
  //
  // Most nodes do not need to use the full height of the tower, since the
  // probability of each successive level decreases exponentially. Because
  // these elements are never accessed, they do not need to be allocated.
  // Therefore, when a node is allocated in the arena, its memory footprint
  // is deliberately truncated to not include unneeded tower elements.
  //
  // All accesses to elements should use CAS operations, with no need to lock.
  // pub(super) tower: [Link; self.opts.max_height],
}

impl<T> core::fmt::Debug for Node<T> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let (key_size, height) = decode_key_size_and_height(self.key_size_and_height);
    let (value_offset, value_size) = decode_value_pointer(self.value.0.load(Ordering::Relaxed));
    f.debug_struct("Node")
      .field("value_offset", &value_offset)
      .field("value_size", &value_size)
      .field("key_offset", &self.key_offset)
      .field("key_size", &key_size)
      .field("height", &height)
      .finish()
  }
}

impl<T> Node<T> {
  const SIZE: usize = mem::size_of::<Self>();
  const ALIGN: u32 = mem::align_of::<Self>() as u32;

  #[inline]
  fn full(value_offset: u32, max_height: u8) -> Self {
    Self {
      value: AtomicValuePointer::new(value_offset, 0),
      key_offset: 0,
      key_size_and_height: encode_key_size_and_height(0, max_height),
      trailer: PhantomData,
    }
  }

  #[inline]
  const fn size(max_height: u8) -> usize {
    Self::SIZE + (max_height as usize) * Link::SIZE
  }

  // TODO: remove those functions
  // #[inline]
  // const fn min_cap() -> usize {
  //   (Node::<T>::MAX_NODE_SIZE * 2) as usize
  // }

  // #[inline]
  // const fn max_node_size() -> u32 {
  //   Node::<T>::MAX_NODE_SIZE as u32
  // }

  #[inline]
  fn set_value<'a, E>(
    &self,
    arena: &'a Arena,
    trailer: T,
    value_size: u32,
    f: &impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(), Either<E, Error>> {
    let mut bytes = arena
      .alloc_aligned_bytes::<T>(value_size)
      .map_err(|e| Either::Right(e.into()))?;
    let trailer_ptr = bytes.as_mut_ptr().cast::<T>();
    let trailer_offset = bytes.offset();
    let value_offset = trailer_offset + mem::size_of::<T>();

    let mut oval = VacantBuffer::new(value_size as usize, value_offset as u32, unsafe {
      arena.get_bytes_mut(value_offset, value_size as usize)
    });
    f(&mut oval).map_err(Either::Left)?;

    let remaining = oval.remaining();
    let mut discard = 0;
    if remaining != 0
      && unsafe { !arena.dealloc((value_offset + oval.len()) as u32, remaining as u32) }
    {
      discard += remaining;
    }

    bytes.detach();
    unsafe {
      trailer_ptr.write(trailer);
    }

    if discard != 0 {
      arena.increase_discarded(discard as u32);
    }

    let (old_offset, old_size) = self.value.swap(trailer_offset as u32, value_size);

    // on success, which means that old value is removed, we need to dealloc the old value
    unsafe {
      arena.dealloc(old_offset, (mem::size_of::<T>() as u32) + old_size);
    }

    Ok(())
  }

  #[inline]
  fn clear_value(
    &self,
    arena: &Arena,
    success: Ordering,
    failure: Ordering,
  ) -> Result<(), (u32, u32)> {
    self
      .value
      .compare_remove(success, failure)
      .map(|(offset, size)| {
        if size != u32::MAX {
          unsafe {
            arena.dealloc(offset, (mem::size_of::<T>() as u32) + size);
          }
        } else {
          unsafe {
            arena.dealloc(offset, mem::size_of::<T>() as u32);
          }
        }
      })
  }
}

impl<T> Node<T> {
  #[inline]
  const fn key_size(&self) -> u32 {
    decode_key_size_and_height(self.key_size_and_height).0
  }

  #[inline]
  const fn height(&self) -> u8 {
    decode_key_size_and_height(self.key_size_and_height).1
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  const unsafe fn get_key<'a, 'b: 'a>(&'a self, arena: &'b Arena) -> &'b [u8] {
    arena.get_bytes(self.key_offset as usize, self.key_size() as usize)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_value<'a, 'b: 'a>(&'a self, arena: &'b Arena) -> Option<&'b [u8]> {
    let (offset, len) = self.value.load(Ordering::Acquire);

    if len == u32::MAX {
      return None;
    }
    let align_offset = Self::align_offset(offset);
    Some(arena.get_bytes(align_offset as usize + mem::size_of::<T>(), len as usize))
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_value_by_offset<'a, 'b: 'a>(
    &'a self,
    arena: &'b Arena,
    offset: u32,
    len: u32,
  ) -> Option<&'b [u8]> {
    if len == u32::MAX {
      return None;
    }
    let align_offset = Self::align_offset(offset);
    Some(arena.get_bytes(align_offset as usize + mem::size_of::<T>(), len as usize))
  }

  #[inline]
  const fn align_offset(current_offset: u32) -> u32 {
    let alignment = mem::align_of::<T>() as u32;
    (current_offset + alignment - 1) & !(alignment - 1)
  }
}

impl<T: Copy> Node<T> {
  #[inline]
  unsafe fn get_trailer<'a, 'b: 'a>(&'a self, arena: &'b Arena) -> T {
    let (offset, _) = self.value.load(Ordering::Acquire);
    *arena.get_aligned_pointer(offset as usize)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_trailer_by_offset<'a, 'b: 'a>(&'a self, arena: &'b Arena, offset: u32) -> T {
    *arena.get_aligned_pointer::<T>(offset as usize)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_value_and_trailer<'a, 'b: 'a>(&'a self, arena: &'b Arena) -> (T, Option<&'b [u8]>) {
    let (offset, len) = self.value.load(Ordering::Acquire);
    let ptr = arena.get_aligned_pointer(offset as usize);
    #[cfg(not(feature = "unaligned"))]
    let trailer = *ptr;

    if len == u32::MAX {
      return (trailer, None);
    }

    let value_offset = arena.offset(ptr as _) + mem::size_of::<T>();
    (trailer, Some(arena.get_bytes(value_offset, len as usize)))
  }
}

/// A fast, cocnurrent map implementation based on skiplist that supports forward
/// and backward iteration. Keys and values are immutable once added to the skipmap and
/// deletion is not supported. Instead, higher-level code is expected to add new
/// entries that shadow existing entries and perform deletion via tombstones. It
/// is up to the user to process these shadow entries and tombstones
/// appropriately during retrieval.
#[derive(Debug)]
pub struct SkipMap<T = u64, C = Ascend> {
  arena: Arena,
  meta: NonNull<Meta>,
  head: NodePtr<T>,
  tail: NodePtr<T>,
  data_offset: u32,
  opts: Options,
  /// If set to true by tests, then extra delays are added to make it easier to
  /// detect unusual race conditions.
  #[cfg(all(test, feature = "std"))]
  yield_now: bool,

  cmp: C,
}

// Safety: SkipMap is Sync and Send
unsafe impl<T: Send, C: Comparator + Send> Send for SkipMap<T, C> {}
unsafe impl<T: Sync, C: Comparator + Sync> Sync for SkipMap<T, C> {}

impl<T, C: Clone> Clone for SkipMap<T, C> {
  fn clone(&self) -> Self {
    Self {
      arena: self.arena.clone(),
      meta: self.meta,
      head: self.head,
      tail: self.tail,
      data_offset: self.data_offset,
      opts: self.opts,
      #[cfg(all(test, feature = "std"))]
      yield_now: self.yield_now,
      cmp: self.cmp.clone(),
    }
  }
}

impl<T, C> Drop for SkipMap<T, C> {
  fn drop(&mut self) {
    if self.arena.refs() == 1 && !self.opts.unify() {
      unsafe {
        let _ = Box::from_raw(self.meta.as_ptr());
      }
    }
  }
}

impl<T, C> SkipMap<T, C> {
  fn new_in(arena: Arena, cmp: C, opts: Options) -> Result<Self, Error> {
    let data_offset = Self::check_capacity(&arena, opts.max_height().into())?;

    if arena.read_only() {
      let (meta, head, tail) = Self::get_pointers(&arena);
      return Ok(Self::construct(
        arena,
        meta,
        head,
        tail,
        data_offset,
        opts,
        cmp,
      ));
    }

    let meta = if opts.unify() {
      Self::allocate_meta(&arena, opts.magic_version())?
    } else {
      unsafe {
        NonNull::new_unchecked(Box::into_raw(Box::new(Meta {
          max_version: AtomicU64::new(0),
          min_version: AtomicU64::new(0),
          height: AtomicU8::new(1),
          len: AtomicU32::new(0),
          magic_version: opts.magic_version(),
          reserved_byte: 0,
        })))
      }
    };

    let max_height: u8 = opts.max_height().into();
    let head = Self::allocate_full_node(&arena, max_height)?;
    let tail = Self::allocate_full_node(&arena, max_height)?;

    // Safety:
    // We will always allocate enough space for the head node and the tail node.
    unsafe {
      // Link all head/tail levels together.
      for i in 0..(max_height as usize) {
        let head_link = head.tower(&arena, i);
        let tail_link = tail.tower(&arena, i);
        head_link.next_offset.store(tail.offset, Ordering::Relaxed);
        tail_link.prev_offset.store(head.offset, Ordering::Relaxed);
      }
    }

    Ok(Self::construct(
      arena,
      meta,
      head,
      tail,
      data_offset,
      opts,
      cmp,
    ))
  }

  /// Checks if the arena has enough capacity to store the skiplist,
  /// and returns the data offset.
  #[inline]
  const fn check_capacity(arena: &Arena, max_height: u8) -> Result<u32, Error> {
    let offset = arena.data_offset();

    let alignment = mem::align_of::<Meta>();
    let meta_offset = (offset + alignment - 1) & !(alignment - 1);
    let meta_end = meta_offset + mem::size_of::<Meta>();

    let alignment = mem::align_of::<Node<T>>();
    let head_offset = (meta_end + alignment - 1) & !(alignment - 1);
    let head_end =
      head_offset + mem::size_of::<Node<T>>() + mem::size_of::<Link>() * max_height as usize;

    let trailer_alignment = mem::align_of::<T>();
    let trailer_size = mem::size_of::<T>();
    let trailer_end = if trailer_size != 0 {
      let trailer_offset = (head_end + trailer_alignment - 1) & !(trailer_alignment - 1);
      trailer_offset + trailer_size
    } else {
      head_end
    };

    let tail_offset = (trailer_end + alignment - 1) & !(alignment - 1);
    let tail_end =
      tail_offset + mem::size_of::<Node<T>>() + mem::size_of::<Link>() * max_height as usize;

    let trailer_end = if trailer_size != 0 {
      let trailer_offset = (tail_end + trailer_alignment - 1) & !(trailer_alignment - 1);
      trailer_offset + trailer_size
    } else {
      tail_end
    };
    if trailer_end > arena.capacity() {
      return Err(Error::ArenaTooSmall);
    }

    Ok(trailer_end as u32)
  }

  /// Allocates a `Node`, key, trailer and value
  fn allocate_entry_node<'a, 'b: 'a, E>(
    &'a self,
    height: u32,
    trailer: T,
    key_size: u32,
    kf: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
    value_size: u32,
    vf: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(NodePtr<T>, Deallocator), Either<E, Error>> {
    self
      .check_node_size(height, key_size, value_size)
      .map_err(Either::Right)?;

    unsafe {
      let mut node = self
        .arena
        .alloc_aligned_bytes::<Node<T>>(height * Link::SIZE as u32)
        .map_err(|e| Either::Right(e.into()))?;
      let node_ptr = node.as_mut_ptr().cast::<Node<T>>();
      let node_offset = node.offset();

      let mut key = self
        .arena
        .alloc_bytes(key_size)
        .map_err(|e| Either::Right(e.into()))?;
      let key_offset = key.offset();
      let key_cap = key.capacity();
      let mut trailer_and_value = self
        .arena
        .alloc_aligned_bytes::<T>(value_size)
        .map_err(|e| Either::Right(e.into()))?;
      let trailer_offset = trailer_and_value.offset();
      let trailer_ptr = trailer_and_value.as_mut_ptr().cast::<T>();
      trailer_ptr.write(trailer);

      let value_offset = (trailer_offset + mem::size_of::<T>()) as u32;

      // Safety: the node is well aligned
      let node_ref = &mut *node_ptr;
      node_ref.value = AtomicValuePointer::new(trailer_offset as u32, value_size);
      node_ref.key_offset = key_offset as u32;
      node_ref.key_size_and_height = encode_key_size_and_height(key_cap as u32, height as u8);
      key.detach();
      let (_, key_deallocate_info) = self
        .fill_vacant_key(key_cap as u32, key_offset as u32, kf)
        .map_err(Either::Left)?;
      trailer_and_value.detach();
      let (_, value_deallocate_info) = self
        .fill_vacant_value(
          trailer_offset as u32,
          trailer_and_value.capacity() as u32,
          value_size,
          value_offset,
          vf,
        )
        .map_err(Either::Left)?;
      node.detach();
      Ok((
        NodePtr::new(node_ptr as _, node_offset as u32),
        Deallocator {
          node: Some(Pointer::new(node_offset as u32, node.capacity() as u32)),
          key: Some(key_deallocate_info),
          value: Some(value_deallocate_info),
        },
      ))
    }
  }

  /// Allocates a `Node` and trailer
  fn allocate_node<'a, 'b: 'a, E>(
    &'a self,
    height: u32,
    trailer: T,
    key_offset: u32,
    key_size: u32,
    value_size: u32,
  ) -> Result<(NodePtr<T>, Deallocator), Either<E, Error>> {
    self
      .check_node_size(height, key_size, value_size)
      .map_err(Either::Right)?;

    unsafe {
      let mut node = self
        .arena
        .alloc_aligned_bytes::<Node<T>>(height * Link::SIZE as u32)
        .map_err(|e| Either::Right(e.into()))?;
      let node_ptr = node.as_mut_ptr().cast::<Node<T>>();
      let node_offset = node.offset();

      let mut trailer_ref = self
        .arena
        .alloc::<T>()
        .map_err(|e| Either::Right(e.into()))?;
      let trailer_offset = trailer_ref.offset();
      trailer_ref.write(trailer);

      // Safety: the node is well aligned
      let node_ref = &mut *node_ptr;
      node_ref.value = AtomicValuePointer::new(trailer_offset as u32, value_size);
      node_ref.key_offset = key_offset;
      node_ref.key_size_and_height = encode_key_size_and_height(key_size, height as u8);

      trailer_ref.detach();
      node.detach();
      Ok((
        NodePtr::new(node_ptr as _, node_offset as u32),
        Deallocator {
          node: Some(Pointer::new(node_offset as u32, node.capacity() as u32)),
          key: None,
          value: Some(Pointer::new(
            trailer_offset as u32,
            mem::size_of::<T>() as u32,
          )),
        },
      ))
    }
  }

  /// Allocates a `Node`, key and trailer
  fn allocate_key_node<'a, 'b: 'a, E>(
    &'a self,
    height: u32,
    trailer: T,
    key_size: u32,
    kf: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
    value_size: u32,
  ) -> Result<(NodePtr<T>, Deallocator), Either<E, Error>> {
    self
      .check_node_size(height, key_size, value_size)
      .map_err(Either::Right)?;

    unsafe {
      let mut node = self
        .arena
        .alloc_aligned_bytes::<Node<T>>(height * Link::SIZE as u32)
        .map_err(|e| Either::Right(e.into()))?;
      let node_ptr = node.as_mut_ptr().cast::<Node<T>>();
      let node_offset = node.offset();

      let mut key = self
        .arena
        .alloc_bytes(key_size)
        .map_err(|e| Either::Right(e.into()))?;
      let key_offset = key.offset();
      let key_cap = key.capacity();

      let mut trailer_ref = self
        .arena
        .alloc::<T>()
        .map_err(|e| Either::Right(e.into()))?;
      let trailer_offset = trailer_ref.offset();
      trailer_ref.write(trailer);

      // Safety: the node is well aligned
      let node_ref = &mut *node_ptr;
      node_ref.value = AtomicValuePointer::new(trailer_offset as u32, value_size);
      node_ref.key_offset = key_offset as u32;
      node_ref.key_size_and_height = encode_key_size_and_height(key_cap as u32, height as u8);

      key.detach();
      let (_, key_deallocate_info) = self
        .fill_vacant_key(key_cap as u32, key_offset as u32, kf)
        .map_err(Either::Left)?;

      trailer_ref.detach();
      node.detach();

      Ok((
        NodePtr::new(node_ptr as _, node_offset as u32),
        Deallocator {
          node: Some(Pointer::new(node_offset as u32, node.capacity() as u32)),
          key: Some(key_deallocate_info),
          value: Some(Pointer::new(
            trailer_offset as u32,
            mem::size_of::<T>() as u32,
          )),
        },
      ))
    }
  }

  /// Allocates a `Node`, trailer and value
  fn allocate_value_node<'a, 'b: 'a, E>(
    &'a self,
    height: u32,
    trailer: T,
    key_size: u32,
    key_offset: u32,
    value_size: u32,
    vf: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(NodePtr<T>, Deallocator), Either<E, Error>> {
    self
      .check_node_size(height, key_size, value_size)
      .map_err(Either::Right)?;

    unsafe {
      let mut node = self
        .arena
        .alloc_aligned_bytes::<Node<T>>(height * Link::SIZE as u32)
        .map_err(|e| Either::Right(e.into()))?;
      let node_ptr = node.as_mut_ptr().cast::<Node<T>>();
      let node_offset = node.offset();

      let mut trailer_and_value = self
        .arena
        .alloc_aligned_bytes::<T>(value_size)
        .map_err(|e| Either::Right(e.into()))?;
      let trailer_offset = trailer_and_value.offset();
      let trailer_ptr = trailer_and_value.as_mut_ptr().cast::<T>();
      trailer_ptr.write(trailer);
      let value_offset = (trailer_offset + mem::size_of::<T>()) as u32;

      // Safety: the node is well aligned
      let node_ref = &mut *node_ptr;
      node_ref.value = AtomicValuePointer::new(trailer_offset as u32, value_size);
      node_ref.key_offset = key_offset;
      node_ref.key_size_and_height = encode_key_size_and_height(key_size, height as u8);

      trailer_and_value.detach();
      let (_, value_deallocate_info) = self
        .fill_vacant_value(
          trailer_offset as u32,
          trailer_and_value.capacity() as u32,
          value_size,
          value_offset,
          vf,
        )
        .map_err(Either::Left)?;

      node.detach();

      Ok((
        NodePtr::new(node_ptr as _, node_offset as u32),
        Deallocator {
          node: Some(Pointer::new(node_offset as u32, node.capacity() as u32)),
          key: None,
          value: Some(value_deallocate_info),
        },
      ))
    }
  }

  fn allocate_full_node(arena: &Arena, max_height: u8) -> Result<NodePtr<T>, ArenaError> {
    // Safety: node, links and trailer do not need to be dropped, and they are recoverable.
    unsafe {
      let mut node =
        arena.alloc_aligned_bytes::<Node<T>>(((max_height as usize) * Link::SIZE) as u32)?;

      // Safety: node and trailer do not need to be dropped.
      node.detach();

      let node_ptr = node.as_mut_ptr().cast::<Node<T>>();
      let node_offset = node.offset();

      let trailer_offset = if mem::size_of::<T>() != 0 {
        let mut trailer = arena.alloc::<T>()?;
        trailer.detach();
        trailer.offset()
      } else {
        arena.allocated()
      };

      let node = &mut *node_ptr;
      *node = Node::<T>::full(trailer_offset as u32, max_height);

      Ok(NodePtr::new(node_ptr as _, node_offset as u32))
    }
  }

  #[inline]
  fn allocate_meta(arena: &Arena, magic_version: u16) -> Result<NonNull<Meta>, ArenaError> {
    // Safety: meta does not need to be dropped, and it is recoverable.
    unsafe {
      let mut meta = arena.alloc::<Meta>()?;
      meta.detach();

      meta.write(Meta {
        max_version: AtomicU64::new(0),
        min_version: AtomicU64::new(0),
        height: AtomicU8::new(1),
        len: AtomicU32::new(0),
        magic_version,
        reserved_byte: 0,
      });
      Ok(meta.as_mut_ptr())
    }
  }

  #[inline]
  unsafe fn fill_vacant_key<'a, E>(
    &'a self,
    size: u32,
    offset: u32,
    f: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(u32, Pointer), E> {
    let buf = self.arena.get_bytes_mut(offset as usize, size as usize);
    let mut oval = VacantBuffer::new(size as usize, offset, buf);
    if let Err(e) = f(&mut oval) {
      self.arena.dealloc(offset, size);
      return Err(e);
    }

    let len = oval.len();
    let remaining = oval.remaining();
    if remaining != 0 {
      #[cfg(feature = "tracing")]
      tracing::warn!("vacant value is not fully filled, remaining {remaining} bytes");
      let deallocated = self.arena.dealloc(offset + len as u32, remaining as u32);
      if deallocated {
        return Ok((
          oval.len() as u32,
          Pointer::new(offset, size - remaining as u32),
        ));
      }
    }
    Ok((oval.len() as u32, Pointer::new(offset, size)))
  }

  #[inline]
  unsafe fn fill_vacant_value<'a, E>(
    &'a self,
    offset: u32,
    size: u32,
    value_size: u32,
    value_offset: u32,
    f: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(u32, Pointer), E> {
    let buf = self
      .arena
      .get_bytes_mut(value_offset as usize, value_size as usize);
    let mut oval = VacantBuffer::new(value_size as usize, value_offset, buf);
    if let Err(e) = f(&mut oval) {
      self.arena.dealloc(offset, size);
      return Err(e);
    }

    let len = oval.len();
    let remaining = oval.remaining();
    if remaining != 0 {
      #[cfg(feature = "tracing")]
      tracing::warn!("vacant value is not fully filled, remaining {remaining} bytes");
      let deallocated = self
        .arena
        .dealloc(value_offset + len as u32, remaining as u32);

      if deallocated {
        return Ok((
          oval.len() as u32,
          Pointer::new(offset, size - remaining as u32),
        ));
      }
    }

    Ok((oval.len() as u32, Pointer::new(offset, size)))
  }

  #[inline]
  fn get_pointers(arena: &Arena) -> (NonNull<Meta>, NodePtr<T>, NodePtr<T>) {
    unsafe {
      let offset = arena.data_offset();
      let meta = arena.get_aligned_pointer::<Meta>(offset);

      let offset = arena.offset(meta as _) + mem::size_of::<Meta>();
      let head_ptr = arena.get_aligned_pointer::<Node<T>>(offset);
      let head_offset = arena.offset(head_ptr as _);
      let head = NodePtr::new(head_ptr as _, head_offset as u32);

      let (trailer_offset, _) = head.as_ref().value.load(Ordering::Relaxed);
      let offset = trailer_offset as usize + mem::size_of::<T>();
      let tail_ptr = arena.get_aligned_pointer::<Node<T>>(offset);
      let tail_offset = arena.offset(tail_ptr as _);
      let tail = NodePtr::new(tail_ptr as _, tail_offset as u32);
      (NonNull::new_unchecked(meta as _), head, tail)
    }
  }

  #[inline]
  fn check_node_size(&self, height: u32, key_size: u32, mut value_size: u32) -> Result<(), Error> {
    let max_height: u32 = self.opts.max_height().into();
    if height < 1 || height > max_height {
      panic!("height cannot be less than one or greater than the max height");
    }

    let max_key_size: u32 = self.opts.max_key_size().into();
    if key_size > max_key_size {
      return Err(Error::KeyTooLarge(key_size as u64));
    }

    // if value_size is u32::MAX, it means that the value is removed.
    value_size = if value_size == u32::MAX {
      0
    } else {
      value_size
    };

    if value_size > self.opts.max_value_size() {
      return Err(Error::ValueTooLarge(value_size as u64));
    }

    let entry_size = (value_size as u64 + key_size as u64) + Node::<T>::size(height as u8) as u64;
    if entry_size > u32::MAX as u64 {
      return Err(Error::EntryTooLarge(entry_size));
    }

    Ok(())
  }

  #[inline]
  fn construct(
    arena: Arena,
    meta: NonNull<Meta>,
    head: NodePtr<T>,
    tail: NodePtr<T>,
    data_offset: u32,
    opts: Options,
    cmp: C,
  ) -> Self {
    Self {
      arena,
      meta,
      head,
      tail,
      data_offset,
      opts,
      #[cfg(all(test, feature = "std"))]
      yield_now: false,
      cmp,
    }
  }

  #[inline]
  const fn meta(&self) -> &Meta {
    // Safety: the pointer is well aligned and initialized.
    unsafe { self.meta.as_ref() }
  }
}

impl<T: Trailer, C> SkipMap<T, C> {
  fn new_node<'a, 'b: 'a, E>(
    &'a self,
    key: &Key<'a, 'b>,
    trailer: T,
    value_size: u32,
    f: &impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<(NodePtr<T>, u32, Deallocator), Either<E, Error>> {
    let height = super::random_height(self.opts.max_height().into());
    let (nd, deallocator) = match key {
      Key::Occupied(key) => self.allocate_entry_node(
        height,
        trailer,
        key.len() as u32,
        |buf| {
          buf.write(key).unwrap();
          Ok(())
        },
        value_size,
        f,
      )?,
      Key::Vacant(key) => {
        self.allocate_value_node(height, trailer, key.len() as u32, key.offset, value_size, f)?
      }
      Key::Pointer { offset, len, .. } => {
        self.allocate_value_node(height, trailer, *len, *offset, value_size, f)?
      }
      Key::Remove(key) => self.allocate_key_node(
        height,
        trailer,
        key.len() as u32,
        |buf| {
          buf.write(key).expect("buffer must be large enough for key");
          Ok(())
        },
        REMOVE,
      )?,
      Key::RemoveVacant(key) => {
        self.allocate_node(height, trailer, key.offset, key.len() as u32, REMOVE)?
      }
      Key::RemovePointer { offset, len, .. } => {
        self.allocate_node(height, trailer, *offset, *len, REMOVE)?
      }
    };

    // Try to increase self.height via CAS.
    let mut list_height = self.height();
    while height as u8 > list_height {
      match self.meta().height.compare_exchange_weak(
        list_height,
        height as u8,
        Ordering::SeqCst,
        Ordering::Acquire,
      ) {
        // Successfully increased skiplist.height.
        Ok(_) => break,
        Err(h) => list_height = h,
      }
    }
    Ok((nd, height, deallocator))
  }
}

impl<T: Trailer, C: Comparator> SkipMap<T, C> {
  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_prev(&self, nd: NodePtr<T>, height: usize) -> NodePtr<T> {
    if nd.is_null() {
      return NodePtr::NULL;
    }

    let offset = nd.prev_offset(&self.arena, height);
    let ptr = self.arena.get_pointer(offset as usize);
    NodePtr::new(ptr as _, offset)
  }

  /// ## Safety
  ///
  /// - The caller must ensure that the node is allocated by the arena.
  #[inline]
  unsafe fn get_next(&self, nptr: NodePtr<T>, height: usize) -> NodePtr<T> {
    if nptr.is_null() {
      return NodePtr::NULL;
    }
    let offset = nptr.next_offset(&self.arena, height);
    let ptr = self.arena.get_pointer(offset as usize);
    NodePtr::new(ptr as _, offset)
  }

  /// Returns the first entry in the map.
  fn first_in(&self, version: u64) -> Option<NodePtr<T>> {
    // Safety: head node was definitely allocated by self.arena
    let nd = unsafe { self.get_next(self.head, 0) };

    if nd.is_null() || nd.ptr == self.tail.ptr {
      return None;
    }

    unsafe {
      let node = nd.as_ref();
      let curr_key = node.get_key(&self.arena);
      self.ge(version, curr_key)
    }
  }

  /// Returns the last entry in the map.
  fn last_in(&self, version: u64) -> Option<NodePtr<T>> {
    // Safety: tail node was definitely allocated by self.arena
    let nd = unsafe { self.get_prev(self.tail, 0) };

    if nd.is_null() || nd.ptr == self.head.ptr {
      return None;
    }

    unsafe {
      let node = nd.as_ref();
      let curr_key = node.get_key(&self.arena);
      self.le(version, curr_key)
    }
  }

  /// Returns the entry greater or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, key is equal to k1, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  fn gt<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> Option<NodePtr<T>> {
    unsafe {
      let (n, _) = self.find_near(u64::MIN, key, false, false); // find the key with the max version.

      let n = n?;

      if n.is_null() || n.ptr == self.tail.ptr {
        return None;
      }

      self.find_next_max_version(n, version)
    }
  }

  /// Returns the entry less than the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k2 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  fn lt<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> Option<NodePtr<T>> {
    unsafe {
      let (n, _) = self.find_near(u64::MAX, key, true, false); // find less or equal.

      let n = n?;
      if n.is_null() || n.ptr == self.head.ptr {
        return None;
      }

      self.find_prev_max_version(n, version)
    }
  }

  /// Returns the entry greater than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, key is equal to k1, then the entry contains k1 will be returned.
  /// - If k1 < k2 < k3, and k1 < key < k2, then the entry contains k2 will be returned.
  fn ge<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> Option<NodePtr<T>> {
    unsafe {
      // TODO: optimize find_near implementation, so that we can directly use version instead of u64::MIN
      let (n, _) = self.find_near(u64::MAX, key, false, true); // find the key with the max version.

      let n = n?;

      if n.is_null() || n.ptr == self.tail.ptr {
        return None;
      }

      self.find_next_max_version(n, version)
    }
  }

  /// Returns the entry less than or equal to the given key, if it exists.
  ///
  /// e.g.
  ///
  /// - If k1 < k2 < k3, and key is equal to k3, then the entry contains k3 will be returned.
  /// - If k1 < k2 < k3, and k2 < key < k3, then the entry contains k2 will be returned.
  fn le<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> Option<NodePtr<T>> {
    unsafe {
      let (n, _) = self.find_near(u64::MIN, key, true, true); // find less or equal.

      let n = n?;
      if n.is_null() || n.ptr == self.head.ptr {
        return None;
      }

      self.find_prev_max_version(n, version)
    }
  }

  unsafe fn find_prev_max_version(&self, mut curr: NodePtr<T>, version: u64) -> Option<NodePtr<T>> {
    let mut prev = self.get_prev(curr, 0);

    loop {
      let curr_node = curr.as_ref();
      let curr_key = curr_node.get_key(&self.arena);
      // if the current version is greater than the given version, we should return.
      let version_cmp = curr_node.get_trailer(&self.arena).version().cmp(&version);
      if version_cmp == cmp::Ordering::Greater {
        return None;
      }

      if prev.is_null() || prev.ptr == self.head.ptr {
        if let cmp::Ordering::Less | cmp::Ordering::Equal = version_cmp {
          return Some(curr);
        }

        return None;
      }

      let prev_node = prev.as_ref();
      let prev_key = prev_node.get_key(&self.arena);
      if self.cmp.compare(prev_key, curr_key) == cmp::Ordering::Less {
        return Some(curr);
      }

      let version_cmp = prev_node.get_trailer(&self.arena).version().cmp(&version);

      if version_cmp == cmp::Ordering::Equal {
        return Some(prev);
      }

      if version_cmp == cmp::Ordering::Greater {
        return Some(curr);
      }

      curr = prev;
      prev = self.get_prev(curr, 0);
    }
  }

  unsafe fn find_next_max_version(&self, mut curr: NodePtr<T>, version: u64) -> Option<NodePtr<T>> {
    let mut next = self.get_next(curr, 0);

    loop {
      let curr_node = curr.as_ref();
      let curr_key = curr_node.get_key(&self.arena);
      // if the current version is less or equal to the given version, we should return.
      let version_cmp = curr_node.get_trailer(&self.arena).version().cmp(&version);
      if let cmp::Ordering::Less | cmp::Ordering::Equal = version_cmp {
        return Some(curr);
      }

      if next.is_null() || next.ptr == self.head.ptr {
        if let cmp::Ordering::Less | cmp::Ordering::Equal = version_cmp {
          return Some(curr);
        }

        return None;
      }

      let next_node = next.as_ref();
      let next_key = next_node.get_key(&self.arena);
      let version_cmp = next_node.get_trailer(&self.arena).version().cmp(&version);
      if self.cmp.compare(next_key, curr_key) == cmp::Ordering::Greater {
        if let cmp::Ordering::Less | cmp::Ordering::Equal = version_cmp {
          return Some(curr);
        }

        return None;
      }

      if let cmp::Ordering::Less | cmp::Ordering::Equal = version_cmp {
        if next.ptr == self.tail.ptr {
          return None;
        }

        return Some(next);
      }

      curr = next;
      next = self.get_next(curr, 0);
    }
  }

  /// finds the node near to key.
  /// If less=true, it finds rightmost node such that node.key < key (if allow_equal=false) or
  /// node.key <= key (if allow_equal=true).
  /// If less=false, it finds leftmost node such that node.key > key (if allow_equal=false) or
  /// node.key >= key (if allow_equal=true).
  /// Returns the node found. The bool returned is true if the node has key equal to given key.
  unsafe fn find_near(
    &self,
    version: u64,
    key: &[u8],
    less: bool,
    allow_equal: bool,
  ) -> (Option<NodePtr<T>>, bool) {
    let mut x = self.head;
    let mut level = self.height() as usize - 1;

    loop {
      // Assume x.key < key.
      let next = self.get_next(x, level);
      if next.is_null() || next.ptr == self.tail.ptr {
        // x.key < key < END OF LIST
        if level > 0 {
          // Can descend further to iterate closer to the end.
          level -= 1;
          continue;
        }

        // level == 0. Can't descend further. Let's return something that makes sense.
        if !less {
          return (None, false);
        }

        // Try to return x. Make sure it is not a head node.
        if x.ptr == self.head.ptr {
          return (None, false);
        }

        return (Some(x), false);
      }

      let next_node = next.as_ref();
      let next_key = next_node.get_key(&self.arena);
      let cmp = self
        .cmp
        .compare(key, next_key)
        .then_with(|| next_node.get_trailer(&self.arena).version().cmp(&version));

      match cmp {
        cmp::Ordering::Greater => {
          // x.key < next.key < key. We can continue to move right.
          x = next;
          continue;
        }
        cmp::Ordering::Equal => {
          // x.key < key == next.key.
          if allow_equal {
            return (Some(next), true);
          }

          if !less {
            // We want >, so go to base level to grab the next bigger node.
            return (Some(self.get_next(next, 0)), false);
          }

          // We want <. If not base level, we should go closer in the next level.
          if level > 0 {
            level -= 1;
            continue;
          }

          // On base level, Return x.
          return (Some(x), false);
        }
        // In other words, x.key < key < next.
        cmp::Ordering::Less => {
          if level > 0 {
            level -= 1;
            continue;
          }

          // On base level. Need to return something.
          if !less {
            return (Some(next), false);
          }

          // Try to return x. Make sure it is not a head node.
          if x.ptr == self.head.ptr {
            return (None, false);
          }

          return (Some(x), false);
        }
      }
    }
  }

  /// ## Safety:
  /// - All of splices in the inserter must be contains node ptrs are allocated by the current skip map.
  unsafe fn find_splice<'a, 'b: 'a>(
    &'a self,
    version: u64,
    key: &'b [u8],
    ins: &mut Inserter<T>,
    returned_when_found: bool,
  ) -> (bool, Option<Pointer>, Option<NodePtr<T>>) {
    let list_height = self.height() as u32;
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

        if spl.prev.ptr != self.head.ptr && !self.key_is_after_node(spl.prev, version, key) {
          // Key lies before splice.
          level = list_height as usize;
          break;
        }

        if spl.next.ptr != self.tail.ptr && !self.key_is_after_node(spl.next, version, key) {
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
    let mut found_key = None;
    for lvl in (0..level).rev() {
      let mut fr = self.find_splice_for_level(version, key, lvl, prev);
      if fr.splice.next.is_null() {
        fr.splice.next = self.tail;
      }
      found = fr.found;
      if let Some(key) = fr.found_key {
        found_key.get_or_insert(key);
      }
      if found && returned_when_found {
        return (found, found_key, fr.curr);
      }
      ins.spl[lvl] = fr.splice;
    }

    (found, found_key, None)
  }

  /// ## Safety
  /// - `level` is less than `MAX_HEIGHT`.
  /// - `start` must be allocated by self's arena.
  unsafe fn find_splice_for_level(
    &self,
    version: u64,
    key: &[u8],
    level: usize,
    start: NodePtr<T>,
  ) -> FindResult<T> {
    let mut prev = start;

    loop {
      // Assume prev.key < key.
      let next = self.get_next(prev, level);
      if next.ptr == self.tail.ptr {
        // Tail node, so done.
        return FindResult {
          splice: Splice { prev, next },
          found: false,
          found_key: None,
          curr: None,
        };
      }

      // offset is not zero, so we can safely dereference the next node ptr.
      let next_node = next.as_ref();
      let next_key = next_node.get_key(&self.arena);

      let cmp = self.cmp.compare(key, next_key);

      let mut found_key = None;

      match cmp {
        cmp::Ordering::Equal => {
          found_key = Some(Pointer {
            offset: next_node.key_offset,
            size: next_node.key_size(),
            height: Some(next_node.height()),
          });
        }
        cmp::Ordering::Greater => {
          if next_key.starts_with(key) {
            found_key = Some(Pointer {
              offset: next_node.key_offset,
              size: key.len() as u32,
              height: Some(next_node.height()),
            });
          }
        }
        _ => {}
      }

      match cmp.then_with(|| next_node.get_trailer(&self.arena).version().cmp(&version)) {
        // We are done for this level, since prev.key < key < next.key.
        cmp::Ordering::Less => {
          return FindResult {
            splice: Splice { prev, next },
            found: false,
            found_key,
            curr: None,
          };
        }
        // Keep moving right on this level.
        cmp::Ordering::Greater => prev = next,
        cmp::Ordering::Equal => {
          return FindResult {
            splice: Splice { prev, next },
            found: true,
            found_key,
            curr: Some(next),
          };
        }
      }
    }
  }

  /// ## Safety
  /// - The caller must ensure that the node is allocated by the arena.
  /// - The caller must ensure that the node is not null.
  unsafe fn key_is_after_node(&self, nd: NodePtr<T>, version: u64, key: &[u8]) -> bool {
    let nd = &*nd.ptr;
    let nd_key = self
      .arena
      .get_bytes(nd.key_offset as usize, nd.key_size() as usize);

    match self
      .cmp
      .compare(nd_key, key)
      // .then_with(|| version.cmp(&nd.version))
    {
      cmp::Ordering::Less => true,
      cmp::Ordering::Greater => false,
      cmp::Ordering::Equal => {
        matches!(version.cmp(&nd.get_trailer(&self.arena).version()), cmp::Ordering::Less)
      }
    }
  }

  fn fetch_vacant_key<'a, 'b: 'a, E>(
    &'a self,
    key_size: u32,
    key: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<VacantBuffer<'a>, Either<E, Error>> {
    let (key_offset, key_size) = self
      .arena
      .alloc_bytes(key_size)
      .map(|mut b| {
        b.detach();
        (b.offset(), b.capacity())
      })
      .map_err(|e| Either::Right(e.into()))?;

    let mut vk = unsafe {
      VacantBuffer::new(
        key_size,
        key_offset as u32,
        self.arena.get_bytes_mut(key_offset, key_size),
      )
    };
    key(&mut vk)
      .map_err(|e| {
        unsafe {
          self.arena.dealloc(key_offset as u32, key_size as u32);
        }
        Either::Left(e)
      })
      .map(|_| vk)
  }

  #[allow(clippy::too_many_arguments)]
  fn update<'a, 'b: 'a, E>(
    &'a self,
    trailer: T,
    key: Key<'a, 'b>,
    value_size: u32,
    f: impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
    success: Ordering,
    failure: Ordering,
    ins: &mut Inserter<T>,
    upsert: bool,
  ) -> Result<UpdateOk<'a, 'b, T, C>, Either<E, Error>> {
    let version = trailer.version();

    // Safety: a fresh new Inserter, so safe here
    let found_key = unsafe {
      let (found, found_key, ptr) = self.find_splice(version, key.as_ref(), ins, true);
      if found {
        let node_ptr = ptr.expect("the NodePtr cannot be `None` when we found");
        let old = VersionedEntryRef::from_node(node_ptr, self);

        key.on_fail(&self.arena);

        if upsert {
          return self.upsert(
            old, node_ptr, &key, trailer, value_size, &f, success, failure,
          );
        }

        return Ok(Either::Left(if old.is_removed() {
          None
        } else {
          Some(old)
        }));
      }

      found_key
    };

    #[cfg(all(test, feature = "std"))]
    if self.yield_now {
      // Add delay to make it easier to test race between this thread
      // and another thread that sees the intermediate state between
      // finding the splice and using it.
      std::thread::yield_now();
    }

    let mut k = match found_key {
      None => key,
      Some(k) => {
        if key.is_remove() {
          Key::RemovePointer {
            arena: &self.arena,
            offset: k.offset,
            len: k.size,
          }
        } else {
          Key::Pointer {
            arena: &self.arena,
            offset: k.offset,
            len: k.size,
          }
        }
      }
    };

    let (nd, height, mut deallocator) =
      self.new_node(&k, trailer, value_size, &f).map_err(|e| {
        k.on_fail(&self.arena);
        e
      })?;

    // We always insert from the base level and up. After you add a node in base
    // level, we cannot create a node in the level above because it would have
    // discovered the node in the base level.
    let mut invalid_data_splice = false;

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
              let _ = next.cas_prev_offset(
                &self.arena,
                i,
                next_prev_offset,
                prev_offset,
                Ordering::SeqCst,
                Ordering::Acquire,
              );
            }
          }

          match prev.cas_next_offset(
            &self.arena,
            i,
            next.offset,
            nd.offset,
            Ordering::SeqCst,
            Ordering::Acquire,
          ) {
            Ok(_) => {
              // Managed to insert nd between prev and next, so update the next
              // node's prev link and go to the next level.
              #[cfg(all(test, feature = "std"))]
              if self.yield_now {
                // Add delay to make it easier to test race between this thread
                // and another thread that sees the intermediate state between
                // setting next and setting prev.
                std::thread::yield_now();
              }

              let _ = next.cas_prev_offset(
                &self.arena,
                i,
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
              let fr = self.find_splice_for_level(trailer.version(), k.as_ref(), i, prev);
              if fr.found {
                if i != 0 {
                  panic!("how can another thread have inserted a node at a non-base level?");
                }

                let node_ptr = fr
                  .curr
                  .expect("the current should not be `None` when we found");
                let old = VersionedEntryRef::from_node(node_ptr, self);

                k.on_fail(&self.arena);

                if upsert {
                  deallocator.dealloc(&self.arena);
                  return self.upsert(old, node_ptr, &k, trailer, value_size, &f, success, failure);
                }

                deallocator.dealloc(&self.arena);
                return Ok(Either::Left(if old.is_removed() {
                  None
                } else {
                  Some(old)
                }));
              }

              if let Some(p) = fr.found_key {
                k.on_fail(&self.arena);
                let node = nd.as_mut();
                node.key_offset = p.offset;
                node.key_size_and_height = encode_key_size_and_height(p.size, p.height.unwrap());
                deallocator.key = None;
                k = Key::Pointer {
                  arena: &self.arena,
                  offset: p.offset,
                  len: p.size,
                };
              }

              invalid_data_splice = true;
              prev = fr.splice.prev;
              next = fr.splice.next;
            }
          }
        }
      }
    }

    // If we had to recompute the splice for a level, invalidate the entire
    // cached splice.
    if invalid_data_splice {
      ins.height = 0;
    } else {
      // The splice was valid. We inserted a node between spl[i].prev and
      // spl[i].next. Optimistically update spl[i].prev for use in a subsequent
      // call to add.
      for i in 0..(height as usize) {
        ins.spl[i].prev = nd;
      }
    }
    self.meta().increase_len();
    self.meta().update_max_version(version);
    self.meta().update_min_version(version);

    Ok(Either::Left(None))
  }

  #[allow(clippy::too_many_arguments)]
  unsafe fn upsert<'a, 'b: 'a, E>(
    &'a self,
    old: VersionedEntryRef<'a, T, C>,
    node_ptr: NodePtr<T>,
    key: &Key<'a, 'b>,
    trailer: T,
    value_size: u32,
    f: &impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
    success: Ordering,
    failure: Ordering,
  ) -> Result<UpdateOk<'a, 'b, T, C>, Either<E, Error>> {
    match key {
      Key::Occupied(_) | Key::Vacant(_) | Key::Pointer { .. } => node_ptr
        .as_ref()
        .set_value(&self.arena, trailer, value_size, f)
        .map(|_| Either::Left(if old.is_removed() { None } else { Some(old) })),
      Key::Remove(_) | Key::RemoveVacant(_) | Key::RemovePointer { .. } => {
        let node = node_ptr.as_ref();
        let key = node.get_key(&self.arena);
        match node.clear_value(&self.arena, success, failure) {
          Ok(_) => Ok(Either::Left(None)),
          Err((offset, len)) => {
            let trailer = node.get_trailer_by_offset(&self.arena, offset);
            let value = node.get_value_by_offset(&self.arena, offset, len);
            Ok(Either::Right(Err(VersionedEntryRef {
              map: self,
              key,
              trailer,
              value,
              ptr: node_ptr,
            })))
          }
        }
      }
    }
  }
}

/// A helper struct for caching splice information
pub struct Inserter<'a, T> {
  spl: [Splice<T>; super::MAX_HEIGHT],
  height: u32,
  _m: core::marker::PhantomData<&'a ()>,
}

impl<'a, T: Copy> Default for Inserter<'a, T> {
  #[inline]
  fn default() -> Self {
    Self {
      spl: [Splice::default(); super::MAX_HEIGHT],
      height: 0,
      _m: core::marker::PhantomData,
    }
  }
}

#[derive(Debug, Clone, Copy)]
struct Splice<T> {
  prev: NodePtr<T>,
  next: NodePtr<T>,
}

impl<T> Default for Splice<T> {
  #[inline]
  fn default() -> Self {
    Self {
      prev: NodePtr::NULL,
      next: NodePtr::NULL,
    }
  }
}

struct Deallocator {
  node: Option<Pointer>,
  key: Option<Pointer>,
  value: Option<Pointer>,
}

impl Deallocator {
  #[inline]
  fn dealloc(self, arena: &Arena) {
    unsafe {
      if let Some(ptr) = self.node {
        arena.dealloc(ptr.offset, ptr.size);
      }

      if let Some(ptr) = self.key {
        arena.dealloc(ptr.offset, ptr.size);
      }

      if let Some(ptr) = self.value {
        arena.dealloc(ptr.offset, ptr.size);
      }
    }
  }
}

struct Pointer {
  offset: u32,
  size: u32,
  height: Option<u8>,
}

impl Pointer {
  #[inline]
  const fn new(offset: u32, size: u32) -> Self {
    Self {
      offset,
      size,
      height: None,
    }
  }
}

struct FindResult<T> {
  // both key and version are equal.
  found: bool,
  // only key is equal.
  found_key: Option<Pointer>,
  splice: Splice<T>,
  curr: Option<NodePtr<T>>,
}

#[inline]
const fn encode_value_pointer(offset: u32, val_size: u32) -> u64 {
  (val_size as u64) << 32 | offset as u64
}

#[inline]
const fn decode_value_pointer(value: u64) -> (u32, u32) {
  let offset = value as u32;
  let val_size = (value >> 32) as u32;
  (offset, val_size)
}

#[inline]
const fn encode_key_size_and_height(key_size: u32, height: u8) -> u32 {
  // first 27 bits for key_size, last 5 bits for height.
  key_size << 5 | height as u32
}

#[inline]
const fn decode_key_size_and_height(size: u32) -> (u32, u8) {
  let key_size = size >> 5;
  let height = (size & 0b11111) as u8;
  (key_size, height)
}

#[cold]
#[inline(never)]
fn noop<E>(_: &mut VacantBuffer<'_>) -> Result<(), E> {
  Ok(())
}
