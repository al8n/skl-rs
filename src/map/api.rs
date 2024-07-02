use rarena_allocator::ArenaOptions;
use ux2::u27;

use super::*;

impl<T> SkipMap<T> {
  /// Create a new skipmap with default options.
  ///
  /// **Note:** The capacity stands for how many memory allocated,
  /// it does not mean the skiplist can store `cap` entries.
  ///
  ///
  ///
  /// **What the difference between this method and [`SkipMap::mmap_anon`]?**
  ///
  /// 1. This method will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///    Even if we are working with raw pointers with `Box::into_raw`,
  ///    the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///    when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///    especially if you're frequently accessing or modifying it.
  ///
  /// 2. Where as [`SkipMap::mmap_anon`] will use mmap anonymous to require memory from the OS.
  ///    If you require very large contiguous memory regions, `mmap` might be more suitable because
  ///    it's more direct in requesting large chunks of memory from the OS.
  ///
  /// [`SkipMap::mmap_anon`]: #method.mmap_anon
  pub fn new() -> Result<Self, Error> {
    Self::with_options(Options::new())
  }

  /// Like [`SkipMap::new`], but with [`Options`].
  #[inline]
  pub fn with_options(opts: Options) -> Result<Self, Error> {
    Self::with_options_and_comparator(opts, Ascend)
  }

  /// Create a new memory map file backed with default options.
  ///
  /// **Note:** The capacity stands for how many memory mmaped,
  /// it does not mean the skipmap can store `cap` entries.
  ///
  /// `lock`: whether to lock the underlying file or not
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn map_mut<P: AsRef<std::path::Path>>(
    path: P,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
  ) -> std::io::Result<Self> {
    Self::map_mut_with_options(path, Options::new(), open_options, mmap_options)
  }

  /// Like [`SkipMap::map_mut`], but with [`Options`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn map_mut_with_options<P: AsRef<std::path::Path>>(
    path: P,
    opts: Options,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
  ) -> std::io::Result<Self> {
    Self::map_mut_with_options_and_comparator(path, opts, open_options, mmap_options, Ascend)
  }

  /// Open an exist file and mmap it to create skipmap.
  ///
  /// `lock`: whether to lock the underlying file or not
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn map<P: AsRef<std::path::Path>>(
    path: P,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
    magic_version: u16,
  ) -> std::io::Result<Self> {
    Self::map_with_comparator(path, open_options, mmap_options, Ascend, magic_version)
  }

  /// Create a new memory map backed skipmap with default options.
  ///
  /// **What the difference between this method and [`SkipMap::new`]?**
  ///
  /// 1. This method will use mmap anonymous to require memory from the OS directly.
  ///    If you require very large contiguous memory regions, this method might be more suitable because
  ///    it's more direct in requesting large chunks of memory from the OS.
  ///
  /// 2. Where as [`SkipMap::new`] will use an `AlignedVec` ensures we are working within Rust's memory safety guarantees.
  ///    Even if we are working with raw pointers with `Box::into_raw`,
  ///    the backend ARENA will reclaim the ownership of this memory by converting it back to a `Box`
  ///    when dropping the backend ARENA. Since `AlignedVec` uses heap memory, the data might be more cache-friendly,
  ///    especially if you're frequently accessing or modifying it.
  ///
  /// [`SkipMap::new`]: #method.new
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn map_anon(mmap_options: MmapOptions) -> std::io::Result<Self> {
    Self::map_anon_with_options_and_comparator(Options::new(), mmap_options, Ascend)
  }

  /// Like [`SkipMap::map_anon`], but with [`Options`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn map_anon_with_options(opts: Options, mmap_options: MmapOptions) -> std::io::Result<Self> {
    Self::map_anon_with_options_and_comparator(opts, mmap_options, Ascend)
  }
}

impl<T, C> SkipMap<T, C> {
  /// Returns the underlying ARENA allocator used by the skipmap.
  ///
  /// This is a low level API, you should not use this method unless you know what you are doing.
  ///
  /// By default, `skl` does not do any forward and backward compatibility checks when using file backed memory map,
  /// so this will allow the users to access the ARENA allocator directly, and allocate some bytes or structures
  /// to help them implement forward and backward compatibility checks.
  ///
  /// # Example
  ///
  /// ```ignore
  /// use skl::{SkipMap, OpenOptions, MmapOptinos};
  ///
  /// const MAGIC_TEXT: u32 = u32::from_le_bytes(*b"al8n");
  ///
  /// struct Meta {
  ///   magic: u32,
  ///   version: u32,
  /// }
  ///
  /// let map = SkipMap::map_mut(
  ///   "/path/to/file",
  ///   OpenOptions::create_new(Some(1000)).read(true).write(true),
  ///   MmapOptions::default(),
  /// ).unwrap();
  /// let arena = map.allocater();
  /// let mut meta = arena.alloc::<Meta>();
  ///
  /// // Safety: Meta does not require any drop, so it is safe to detach it from the ARENA.
  /// unsafe { meta.detach(); }
  /// meta.write(Meta { magic: MAGIC_TEXT, version: 1 }); // now the meta info is persisted to the file.
  /// ```
  #[inline]
  pub const fn allocator(&self) -> &Arena {
    &self.arena
  }

  /// Returns the offset of the data section in the `SkipMap`.
  ///
  /// By default, `SkipMap` will allocate meta, head node, and tail node in the ARENA,
  /// and the data section will be allocated after the tail node.
  ///
  /// This method will return the offset of the data section in the ARENA.
  #[inline]
  pub const fn data_offset(&self) -> usize {
    self.data_offset as usize
  }

  /// Returns the version number of the [`SkipMap`].
  #[inline]
  pub const fn version(&self) -> u16 {
    self.arena.magic_version()
  }

  /// Returns the magic version number of the [`SkipMap`].
  ///
  /// This value can be used to check the compatibility for application using [`SkipMap`].
  #[inline]
  pub const fn magic_version(&self) -> u16 {
    self.meta().magic_version()
  }

  /// Returns the height of the highest tower within any of the nodes that
  /// have ever been allocated as part of this skiplist.
  #[inline]
  pub fn height(&self) -> u8 {
    self.meta().height()
  }

  /// Returns the number of remaining bytes can be allocated by the arena.
  #[inline]
  pub fn remaining(&self) -> usize {
    self.arena.remaining()
  }

  /// Returns the number of bytes that have allocated from the arena.
  #[inline]
  pub fn allocated(&self) -> usize {
    self.arena.allocated()
  }

  /// Returns the capacity of the arena.
  #[inline]
  pub const fn capacity(&self) -> usize {
    self.arena.capacity()
  }

  /// Returns the number of entries in the skipmap.
  #[inline]
  pub fn len(&self) -> usize {
    self.meta().len() as usize
  }

  /// Returns true if the skipmap is empty.
  #[inline]
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// Gets the number of pointers to this `SkipMap` similar to [`Arc::strong_count`](std::sync::Arc::strong_count).
  #[inline]
  pub fn refs(&self) -> usize {
    self.arena.refs()
  }

  /// Returns how many bytes are discarded by the ARENA.
  #[inline]
  pub fn discarded(&self) -> u32 {
    self.arena.discarded()
  }

  /// Returns the maximum version of all entries in the map.
  #[inline]
  pub fn max_version(&self) -> u64 {
    self.meta().max_version()
  }

  /// Returns the minimum version of all entries in the map.
  #[inline]
  pub fn min_version(&self) -> u64 {
    self.meta().min_version()
  }

  /// Returns the comparator used to compare keys.
  #[inline]
  pub const fn comparator(&self) -> &C {
    &self.cmp
  }

  /// Like [`SkipMap::new`], but with a custom [`Comparator`].
  #[inline]
  pub fn with_comparator(cmp: C) -> Result<Self, Error> {
    Self::with_options_and_comparator(Options::new(), cmp)
  }

  /// Like [`SkipMap::new`], but with [`Options`] and a custom [`Comparator`].
  #[inline]
  pub fn with_options_and_comparator(opts: Options, cmp: C) -> Result<Self, Error> {
    let arena_opts = ArenaOptions::new()
      .with_capacity(opts.capacity())
      .with_maximum_alignment(Node::<T>::ALIGN as usize)
      .with_unify(opts.unify())
      .with_magic_version(CURRENT_VERSION)
      .with_freelist(opts.freelist());
    let arena = Arena::new(arena_opts);
    Self::new_in(arena, cmp, opts)
  }

  /// Like [`SkipMap::map_mut`], but with a custom [`Comparator`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  #[inline]
  pub fn map_mut_with_comparator<P: AsRef<std::path::Path>>(
    path: P,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
    cmp: C,
  ) -> std::io::Result<Self> {
    Self::map_mut_with_options_and_comparator(path, Options::new(), open_options, mmap_options, cmp)
  }

  /// Like [`SkipMap::map_mut`], but with [`Options`] and a custom [`Comparator`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  #[inline]
  pub fn map_mut_with_options_and_comparator<P: AsRef<std::path::Path>>(
    path: P,
    opts: Options,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
    cmp: C,
  ) -> std::io::Result<Self> {
    let alignment = Node::<T>::ALIGN as usize;
    let arena_opts = ArenaOptions::new()
      .with_maximum_alignment(alignment)
      .with_magic_version(CURRENT_VERSION)
      .with_freelist(opts.freelist());
    let arena = Arena::map_mut(path, arena_opts, open_options, mmap_options)?;
    Self::new_in(arena, cmp, opts.with_unify(true))
      .map_err(invalid_data)
      .and_then(|map| {
        if map.magic_version() != opts.magic_version() {
          Err(bad_magic_version())
        } else if map.version() != CURRENT_VERSION {
          Err(bad_version())
        } else {
          Ok(map)
        }
      })
  }

  /// Like [`SkipMap::map`], but with a custom [`Comparator`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  #[inline]
  pub fn map_with_comparator<P: AsRef<std::path::Path>>(
    path: P,
    open_options: OpenOptions,
    mmap_options: MmapOptions,
    cmp: C,
    magic_version: u16,
  ) -> std::io::Result<Self> {
    let arena = Arena::map(path, open_options, mmap_options, CURRENT_VERSION)?;
    Self::new_in(
      arena,
      cmp,
      Options::new()
        .with_unify(true)
        .with_magic_version(magic_version),
    )
    .map_err(invalid_data)
    .and_then(|map| {
      if map.magic_version() != magic_version {
        Err(bad_magic_version())
      } else if map.version() != CURRENT_VERSION {
        Err(bad_version())
      } else {
        Ok(map)
      }
    })
  }

  /// Like [`SkipMap::map_anon`], but with a custom [`Comparator`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  #[inline]
  pub fn map_anon_with_comparator(mmap_options: MmapOptions, cmp: C) -> std::io::Result<Self> {
    Self::map_anon_with_options_and_comparator(Options::new(), mmap_options, cmp)
  }

  /// Like [`SkipMap::map_anon`], but with [`Options`] and a custom [`Comparator`].
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  #[inline]
  pub fn map_anon_with_options_and_comparator(
    opts: Options,
    mmap_options: MmapOptions,
    cmp: C,
  ) -> std::io::Result<Self> {
    let alignment = Node::<T>::ALIGN as usize;
    let arena_opts = ArenaOptions::new()
      .with_maximum_alignment(alignment)
      .with_unify(opts.unify())
      .with_magic_version(CURRENT_VERSION);
    let arena = Arena::map_anon(arena_opts, mmap_options)?;
    Self::new_in(arena, cmp, opts).map_err(invalid_data)
  }

  /// Clear the skiplist to empty and re-initialize.
  ///
  /// # Safety
  /// - The current pointers get from the ARENA cannot be used anymore after calling this method.
  /// - This method is not thread-safe.
  ///
  /// # Example
  ///
  /// Undefine behavior:
  ///
  /// ```ignore
  /// let map = SkipMap::new(1000).unwrap();
  ///
  /// map.insert(1, b"hello", b"world").unwrap();
  ///
  /// let data = map.get(b"hello").unwrap();
  ///
  /// map.clear().unwrap();
  ///
  /// let w = data[0]; // undefined behavior
  /// ```
  pub unsafe fn clear(&mut self) -> Result<(), Error> {
    self.arena.clear()?;

    let meta = if self.opts.unify() {
      Self::allocate_meta(&self.arena, self.meta().magic_version())?
    } else {
      unsafe {
        let magic_version = self.meta().magic_version();
        let _ = Box::from_raw(self.meta.as_ptr());
        NonNull::new_unchecked(Box::into_raw(Box::new(Meta::new(magic_version))))
      }
    };

    self.meta = meta;

    let max_height: u8 = self.opts.max_height().into();
    let head = Self::allocate_full_node(&self.arena, max_height)?;
    let tail = Self::allocate_full_node(&self.arena, max_height)?;

    // Safety:
    // We will always allocate enough space for the head node and the tail node.
    unsafe {
      // Link all head/tail levels together.
      for i in 0..(max_height as usize) {
        let head_link = head.tower(&self.arena, i);
        let tail_link = tail.tower(&self.arena, i);
        head_link.next_offset.store(tail.offset, Ordering::Relaxed);
        tail_link.prev_offset.store(head.offset, Ordering::Relaxed);
      }
    }

    self.head = head;
    self.tail = tail;
    Ok(())
  }

  /// Flushes outstanding memory map modifications to disk.
  ///
  /// When this method returns with a non-error result,
  /// all outstanding changes to a file-backed memory map are guaranteed to be durably stored.
  /// The file's metadata (including last modification timestamp) may not be updated.
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn flush(&self) -> std::io::Result<()> {
    self.arena.flush()
  }

  /// Asynchronously flushes outstanding memory map modifications to disk.
  ///
  /// This method initiates flushing modified pages to durable storage, but it will not wait for
  /// the operation to complete before returning. The file's metadata (including last
  /// modification timestamp) may not be updated.
  #[cfg(all(feature = "memmap", not(target_family = "wasm")))]
  #[cfg_attr(docsrs, doc(cfg(all(feature = "memmap", not(target_family = "wasm")))))]
  pub fn flush_async(&self) -> std::io::Result<()> {
    self.arena.flush_async()
  }

  #[cfg(all(test, feature = "std"))]
  #[inline]
  pub(crate) fn with_yield_now(mut self) -> Self {
    self.yield_now = true;
    self
  }
}

impl<T: Trailer, C: Comparator> SkipMap<T, C> {
  /// Upserts a new key-value pair if it does not yet exist, if the key with the given version already exists, it will update the value.
  /// Unlike [`insert`](SkipMap::insert), this method will update the value if the key with the given version already exists.
  ///
  /// - Returns `Ok(None)` if the key was successfully inserted.
  /// - Returns `Ok(Some(old))` if the key with the given version already exists and the value is successfully updated.
  pub fn insert<'a, 'b: 'a>(
    &'a self,
    trailer: T,
    key: &'b [u8],
    value: &'b [u8],
  ) -> Result<Option<EntryRef<'a, T, C>>, Error> {
    if self.arena.read_only() {
      return Err(Error::read_only());
    }

    let copy = |buf: &mut VacantBuffer| {
      let _ = buf.write(value);
      Ok(())
    };
    let val_len = value.len() as u32;

    self
      .update::<Infallible>(
        trailer,
        Key::Occupied(key),
        val_len,
        copy,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        true,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
      .map_err(|e| e.expect_right("must be map::Error"))
  }

  /// Upserts a new key if it does not yet exist, if the key with the given version already exists, it will update the value.
  /// Unlike [`get_or_insert_with_value`](SkipMap::get_or_insert_with_value), this method will update the value if the key with the given version already exists.
  ///
  /// This method is useful when you want to insert a key and you know the value size but you do not have the value
  /// at this moment.
  ///
  /// A placeholder will be inserted first, then you will get an [`VacantBuffer`],
  /// and you must fill the buffer with bytes later in the closure.
  ///
  /// - Returns `Ok(None)` if the key was successfully inserted.
  /// - Returns `Ok(Some(old))` if the key with the given version already exists and the value is successfully updated.
  ///
  /// # Example
  ///
  /// ```rust
  /// use skl::SkipMap;
  ///
  /// struct Person {
  ///   id: u32,
  ///   name: String,
  /// }
  ///
  /// impl Person {
  ///   fn encoded_size(&self) -> usize {
  ///     4 + self.name.len()
  ///   }
  /// }
  ///
  ///
  /// let alice = Person {
  ///   id: 1,
  ///   name: "Alice".to_string(),
  /// };
  ///
  /// let encoded_size = alice.encoded_size();
  ///
  /// let l = SkipMap::new().unwrap();
  ///
  /// l.insert_with_value::<core::convert::Infallible>(1, b"alice", encoded_size as u32, |mut val| {
  ///   val.write(&alice.id.to_le_bytes()).unwrap();
  ///   val.write(alice.name.as_bytes()).unwrap();
  ///   Ok(())
  /// })
  /// .unwrap();
  /// ```
  pub fn insert_with_value<'a, 'b: 'a, E>(
    &'a self,
    trailer: T,
    key: &'b [u8],
    value_size: u32,
    f: impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<Option<EntryRef<'a, T, C>>, Either<E, Error>> {
    if self.arena.read_only() {
      return Err(Either::Right(Error::read_only()));
    }

    self
      .update(
        trailer,
        Key::Occupied(key),
        value_size,
        f,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        true,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
  }

  /// Inserts a new key-value pair if it does not yet exist.
  ///
  /// Unlike [`insert`](SkipMap::insert), this method will not update the value if the key with the given version already exists.
  ///
  /// - Returns `Ok(None)` if the key was successfully get_or_inserted.
  /// - Returns `Ok(Some(_))` if the key with the given version already exists.
  pub fn get_or_insert<'a, 'b: 'a>(
    &'a self,
    trailer: T,
    key: &'b [u8],
    value: &'b [u8],
  ) -> Result<Option<EntryRef<'a, T, C>>, Error> {
    if self.arena.read_only() {
      return Err(Error::read_only());
    }

    let copy = |buf: &mut VacantBuffer| {
      let _ = buf.write(value);
      Ok(())
    };
    let val_len = value.len() as u32;

    self
      .update::<Infallible>(
        trailer,
        Key::Occupied(key),
        val_len,
        copy,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        false,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
      .map_err(|e| e.expect_right("must be map::Error"))
  }

  /// Inserts a new key if it does not yet exist.
  ///
  /// Unlike [`insert_with_value`](SkipMap::insert_with_value), this method will not update the value if the key with the given version already exists.
  ///
  /// This method is useful when you want to get_or_insert a key and you know the value size but you do not have the value
  /// at this moment.
  ///
  /// A placeholder will be inserted first, then you will get an [`VacantBuffer`],
  /// and you must fill the buffer with bytes later in the closure.
  ///
  /// - Returns `Ok(None)` if the key was successfully get_or_inserted.
  /// - Returns `Ok(Some(_))` if the key with the given version already exists.
  ///
  /// # Example
  ///
  /// ```rust
  /// use skl::SkipMap;
  ///
  /// struct Person {
  ///   id: u32,
  ///   name: String,
  /// }
  ///
  /// impl Person {
  ///   fn encoded_size(&self) -> usize {
  ///     4 + self.name.len()
  ///   }
  /// }
  ///
  ///
  /// let alice = Person {
  ///   id: 1,
  ///   name: "Alice".to_string(),
  /// };
  ///
  /// let encoded_size = alice.encoded_size();
  ///
  /// let l = SkipMap::new().unwrap();
  ///
  /// l.get_or_insert_with_value::<core::convert::Infallible>(1, b"alice", encoded_size as u32, |mut val| {
  ///   val.write(&alice.id.to_le_bytes()).unwrap();
  ///   val.write(alice.name.as_bytes()).unwrap();
  ///   Ok(())
  /// })
  /// .unwrap();
  /// ```
  pub fn get_or_insert_with_value<'a, 'b: 'a, E>(
    &'a self,
    trailer: T,
    key: &'b [u8],
    value_size: u32,
    f: impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<Option<EntryRef<'a, T, C>>, Either<E, Error>> {
    if self.arena.read_only() {
      return Err(Either::Right(Error::read_only()));
    }

    self
      .update(
        trailer,
        Key::Occupied(key),
        value_size,
        f,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        false,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
  }

  /// Upserts a new key if it does not yet exist, if the key with the given version already exists, it will update the value.
  /// Unlike [`get_or_insert_with`](SkipMap::get_or_insert_with), this method will update the value if the key with the given version already exists.
  ///
  /// This method is useful when you want to insert a key and you know the value size but you do not have the value
  /// at this moment.
  ///
  /// A placeholder will be inserted first, then you will get an [`VacantBuffer`],
  /// and you must fill the buffer with bytes later in the closure.
  ///
  /// - Returns `Ok(None)` if the key was successfully inserted.
  /// - Returns `Ok(Some(old))` if the key with the given version already exists and the value is successfully updated.
  ///
  /// # Example
  ///
  /// ```rust
  /// use skl::{SkipMap, u27};
  ///
  /// struct Person {
  ///   id: u32,
  ///   name: String,
  /// }
  ///
  /// impl Person {
  ///   fn encoded_size(&self) -> usize {
  ///     4 + self.name.len()
  ///   }
  /// }
  ///
  ///
  /// let alice = Person {
  ///   id: 1,
  ///   name: "Alice".to_string(),
  /// };
  ///
  /// let encoded_size = alice.encoded_size();
  ///
  /// let l = SkipMap::new().unwrap();
  ///
  /// l.insert_with::<core::convert::Infallible>(1, u27::new(5), |key| {
  ///   key.write(b"alice").unwrap();
  ///   Ok(())
  /// }, encoded_size as u32, |mut val| {
  ///   val.write(&alice.id.to_le_bytes()).unwrap();
  ///   val.write(alice.name.as_bytes()).unwrap();
  ///   Ok(())
  /// })
  /// .unwrap();
  /// ```
  pub fn insert_with<'a, E>(
    &'a self,
    trailer: T,
    key_size: u27,
    key: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
    val_size: u32,
    val: impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<Option<EntryRef<'a, T, C>>, Either<E, Error>> {
    let vk = self.fetch_vacant_key(u32::from(key_size), key)?;

    self
      .update(
        trailer,
        Key::Vacant(vk),
        val_size,
        val,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        true,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
  }

  /// Inserts a new key if it does not yet exist.
  ///
  /// Unlike [`insert_with`](SkipMap::insert_with), this method will not update the value if the key with the given version already exists.
  ///
  /// This method is useful when you want to get_or_insert a key and you know the value size but you do not have the value
  /// at this moment.
  ///
  /// A placeholder will be inserted first, then you will get an [`VacantBuffer`],
  /// and you must fill the buffer with bytes later in the closure.
  ///
  /// # Example
  ///
  /// ```rust
  /// use skl::{SkipMap, u27};
  ///
  /// struct Person {
  ///   id: u32,
  ///   name: String,
  /// }
  ///
  /// impl Person {
  ///   fn encoded_size(&self) -> usize {
  ///     4 + self.name.len()
  ///   }
  /// }
  ///
  ///
  /// let alice = Person {
  ///   id: 1,
  ///   name: "Alice".to_string(),
  /// };
  ///
  /// let encoded_size = alice.encoded_size();
  ///
  /// let l = SkipMap::new().unwrap();
  ///
  /// l.get_or_insert_with::<core::convert::Infallible>(1, u27::new(5), |key| {
  ///   key.write(b"alice").unwrap();
  ///   Ok(())
  /// }, encoded_size as u32, |mut val| {
  ///   val.write(&alice.id.to_le_bytes()).unwrap();
  ///   val.write(alice.name.as_bytes()).unwrap();
  ///   Ok(())
  /// })
  /// .unwrap();
  /// ```
  pub fn get_or_insert_with<'a, E>(
    &'a self,
    trailer: T,
    key_size: u27,
    key: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
    val_size: u32,
    val: impl Fn(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<Option<EntryRef<'a, T, C>>, Either<E, Error>> {
    let vk = self.fetch_vacant_key(u32::from(key_size), key)?;

    self
      .update(
        trailer,
        Key::Vacant(vk),
        val_size,
        val,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        false,
      )
      .map(|old| {
        old.expect_left("insert must get InsertOk").and_then(|old| {
          if old.is_removed() {
            None
          } else {
            Some(EntryRef(old))
          }
        })
      })
  }

  /// Removes the key-value pair if it exists. A CAS operation will be used to ensure the operation is atomic.
  ///
  /// Unlike [`get_or_remove`](SkipMap::get_or_remove), this method will remove the value if the key with the given version already exists.
  ///
  /// - Returns `Ok(None)`:
  ///   - if the remove operation is successful or the key is marked in remove status by other threads.
  /// - Returns `Ok(Either::Right(current))` if the key with the given version already exists
  ///   and the entry is not successfully removed because of an update on this entry happens in another thread.
  pub fn compare_remove<'a, 'b: 'a>(
    &'a self,
    trailer: T,
    key: &'b [u8],
    success: Ordering,
    failure: Ordering,
  ) -> Result<Option<EntryRef<'a, T, C>>, Error> {
    self
      .update(
        trailer,
        Key::Remove(key),
        0,
        noop::<Infallible>,
        success,
        failure,
        &mut Inserter::default(),
        true,
      )
      .map(|res| match res {
        Either::Left(_) => None,
        Either::Right(res) => match res {
          Ok(old) => {
            if old.is_removed() {
              None
            } else {
              Some(EntryRef(old))
            }
          }
          Err(current) => {
            if current.is_removed() {
              None
            } else {
              Some(EntryRef(current))
            }
          }
        },
      })
      .map_err(|e| e.expect_right("must be map::Error"))
  }

  /// Gets or removes the key-value pair if it exists.
  /// Unlike [`compare_remove`](SkipMap::compare_remove), this method will not remove the value if the key with the given version already exists.
  ///
  /// - Returns `Ok(None)` if the key does not exist.
  /// - Returns `Ok(Some(old))` if the key with the given version already exists.
  pub fn get_or_remove<'a, 'b: 'a>(
    &'a self,
    trailer: T,
    key: &'b [u8],
  ) -> Result<Option<EntryRef<'a, T, C>>, Error> {
    self
      .update(
        trailer,
        Key::Remove(key),
        0,
        noop::<Infallible>,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        false,
      )
      .map(|res| match res {
        Either::Left(old) => match old {
          Some(old) => {
            if old.is_removed() {
              None
            } else {
              Some(EntryRef(old))
            }
          }
          None => None,
        },
        _ => unreachable!("get_or_remove does not use CAS, so it must return `Either::Left`"),
      })
      .map_err(|e| e.expect_right("must be map::Error"))
  }

  /// Gets or removes the key-value pair if it exists.
  /// Unlike [`compare_remove`](SkipMap::compare_remove), this method will not remove the value if the key with the given version already exists.
  ///
  /// - Returns `Ok(None)` if the key does not exist.
  /// - Returns `Ok(Some(old))` if the key with the given version already exists.
  ///
  /// This method is useful when you want to get_or_remove a key and you know the key size but you do not have the key
  /// at this moment.
  ///
  /// A placeholder will be inserted first, then you will get an [`VacantBuffer`],
  /// and you must fill the buffer with bytes later in the closure.
  ///
  /// # Example
  ///
  /// ```rust
  /// use skl::{SkipMap, u27};
  ///
  /// struct Person {
  ///   id: u32,
  ///   name: String,
  /// }
  ///
  /// impl Person {
  ///   fn encoded_size(&self) -> usize {
  ///     4 + self.name.len()
  ///   }
  /// }
  ///
  ///
  /// let alice = Person {
  ///   id: 1,
  ///   name: "Alice".to_string(),
  /// };
  ///
  /// let encoded_size = alice.encoded_size();
  ///
  /// let l = SkipMap::new().unwrap();
  ///
  /// l.get_or_remove_with::<core::convert::Infallible>(1, u27::new(5), |key| {
  ///   key.write(b"alice").unwrap();
  ///   Ok(())
  /// })
  /// .unwrap();
  /// ```
  pub fn get_or_remove_with<'a, 'b: 'a, E>(
    &'a self,
    trailer: T,
    key_size: u27,
    key: impl FnOnce(&mut VacantBuffer<'a>) -> Result<(), E>,
  ) -> Result<Option<EntryRef<'a, T, C>>, Either<E, Error>> {
    let vk = self.fetch_vacant_key(u32::from(key_size), key)?;
    let key = Key::RemoveVacant(vk);
    self
      .update(
        trailer,
        key,
        0,
        noop::<Infallible>,
        Ordering::Relaxed,
        Ordering::Relaxed,
        &mut Inserter::default(),
        false,
      )
      .map(|res| match res {
        Either::Left(old) => match old {
          Some(old) => {
            if old.is_removed() {
              None
            } else {
              Some(EntryRef(old))
            }
          }
          None => None,
        },
        _ => unreachable!("get_or_remove does not use CAS, so it must return `Either::Left`"),
      })
      .map_err(|e| Either::Right(e.expect_right("must be map::Error")))
  }

  /// Returns true if the key exists in the map.
  #[inline]
  pub fn contains_key<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> bool {
    self.get(version, key).is_some()
  }

  /// Returns the first entry in the map.
  pub fn first(&self, version: u64) -> Option<EntryRef<'_, T, C>> {
    self.iter(version).seek_lower_bound(Bound::Unbounded)
  }

  /// Returns the last entry in the map.
  pub fn last(&self, version: u64) -> Option<EntryRef<'_, T, C>> {
    self.iter(version).seek_upper_bound(Bound::Unbounded)
  }

  /// Returns the value associated with the given key, if it exists.
  pub fn get<'a, 'b: 'a>(&'a self, version: u64, key: &'b [u8]) -> Option<EntryRef<'a, T, C>> {
    unsafe {
      let (n, eq) = self.find_near(version, key, false, true); // findLessOrEqual.

      let n = n?;
      let node = n.as_ref();
      let node_key = node.get_key(&self.arena);
      let (trailer, value) = node.get_value_and_trailer(&self.arena);
      if eq {
        return value.map(|val| {
          EntryRef(VersionedEntryRef {
            map: self,
            key: node_key,
            trailer,
            value: Some(val),
            ptr: n,
          })
        });
      }

      if !matches!(self.cmp.compare(key, node_key), cmp::Ordering::Equal) {
        return None;
      }

      if trailer.version() > version {
        return None;
      }

      value.map(|val| {
        EntryRef(VersionedEntryRef {
          map: self,
          key: node_key,
          trailer,
          value: Some(val),
          ptr: n,
        })
      })
    }
  }

  /// Returns an `EntryRef` pointing to the highest element whose key is below the given bound.
  /// If no such element is found then `None` is returned.
  pub fn upper_bound<'a, 'b: 'a>(
    &'a self,
    version: u64,
    upper: Bound<&'b [u8]>,
  ) -> Option<EntryRef<'a, T, C>> {
    self.iter(version).seek_upper_bound(upper)
  }

  /// Returns an `EntryRef` pointing to the lowest element whose key is above the given bound.
  /// If no such element is found then `None` is returned.
  pub fn lower_bound<'a, 'b: 'a>(
    &'a self,
    version: u64,
    lower: Bound<&'b [u8]>,
  ) -> Option<EntryRef<'a, T, C>> {
    self.iter(version).seek_lower_bound(lower)
  }

  /// Returns a new iterator, this iterator will yield the latest version of all entries in the map less or equal to the given version.
  #[inline]
  pub const fn iter(&self, version: u64) -> iterator::Iter<T, C> {
    iterator::Iter::new(version, self)
  }

  /// Returns a new iterator, this iterator will yield all versions for all entries in the map less or equal to the given version.
  #[inline]
  pub const fn iter_all_versions(&self, version: u64) -> iterator::AllVersionsIter<T, C> {
    iterator::AllVersionsIter::new(version, self, true)
  }

  /// Returns a iterator that within the range, this iterator will yield the latest version of all entries in the range less or equal to the given version.
  #[inline]
  pub fn range<'a, Q, R>(&'a self, version: u64, range: R) -> iterator::Iter<'a, T, C, Q, R>
  where
    &'a [u8]: PartialOrd<Q>,
    Q: ?Sized + PartialOrd<&'a [u8]>,
    R: RangeBounds<Q> + 'a,
  {
    iterator::Iter::range(version, self, range)
  }

  /// Returns a iterator that within the range, this iterator will yield all versions for all entries in the range less or equal to the given version.
  #[inline]
  pub fn range_all_versions<'a, Q, R>(
    &'a self,
    version: u64,
    range: R,
  ) -> iterator::AllVersionsIter<'a, T, C, Q, R>
  where
    &'a [u8]: PartialOrd<Q>,
    Q: ?Sized + PartialOrd<&'a [u8]>,
    R: RangeBounds<Q> + 'a,
  {
    iterator::AllVersionsIter::range(version, self, range, true)
  }
}
