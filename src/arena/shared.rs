use ::alloc::alloc;

use core::{
  ops::{Index, IndexMut},
  ptr, slice,
};

use crate::sync::AtomicUsize;

#[derive(Debug)]
struct AlignedVec {
  ptr: ptr::NonNull<u8>,
  cap: usize,
  len: usize,
  align: usize,
}

impl Drop for AlignedVec {
  #[inline]
  fn drop(&mut self) {
    if self.cap != 0 {
      unsafe {
        alloc::dealloc(self.ptr.as_ptr(), self.layout());
      }
    }
  }
}

impl AlignedVec {
  #[inline]
  fn new(capacity: usize, align: usize) -> Self {
    assert!(
      capacity <= Self::max_capacity(align),
      "`capacity` cannot exceed isize::MAX - {}",
      align - 1
    );
    let ptr = unsafe {
      let layout = alloc::Layout::from_size_align_unchecked(capacity, align);
      let ptr = alloc::alloc(layout);
      if ptr.is_null() {
        alloc::handle_alloc_error(layout);
      }
      ptr::NonNull::new_unchecked(ptr)
    };

    unsafe {
      core::ptr::write_bytes(ptr.as_ptr(), 0, capacity);
    }
    Self {
      ptr,
      cap: capacity,
      len: capacity,
      align,
    }
  }

  #[inline]
  const fn max_capacity(align: usize) -> usize {
    isize::MAX as usize - (align - 1)
  }

  #[inline]
  fn layout(&self) -> alloc::Layout {
    unsafe { alloc::Layout::from_size_align_unchecked(self.cap, self.align) }
  }

  #[inline]
  fn as_mut_ptr(&mut self) -> *mut u8 {
    self.ptr.as_ptr()
  }

  #[inline]
  const fn as_slice(&self) -> &[u8] {
    unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
  }

  #[inline]
  fn as_mut_slice(&mut self) -> &mut [u8] {
    unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
  }
}

impl<I: slice::SliceIndex<[u8]>> Index<I> for AlignedVec {
  type Output = <I as slice::SliceIndex<[u8]>>::Output;

  #[inline]
  fn index(&self, index: I) -> &Self::Output {
    &self.as_slice()[index]
  }
}

impl<I: slice::SliceIndex<[u8]>> IndexMut<I> for AlignedVec {
  #[inline]
  fn index_mut(&mut self, index: I) -> &mut Self::Output {
    &mut self.as_mut_slice()[index]
  }
}

enum SharedBackend {
  Vec(AlignedVec),
  #[cfg(feature = "mmap")]
  Mmap {
    buf: *mut memmapix::MmapMut,
    file: std::fs::File,
    lock: bool,
  },
  #[cfg(feature = "mmap")]
  AnonymousMmap(memmapix::MmapMut),
}

pub(super) struct Shared {
  pub(super) refs: AtomicUsize,
  cap: usize,
  backend: SharedBackend,
}

impl Shared {
  pub(super) fn new_vec(cap: usize, align: usize) -> Self {
    let vec = AlignedVec::new(cap, align);
    Self {
      cap: vec.cap,
      backend: SharedBackend::Vec(vec),
      refs: AtomicUsize::new(1),
    }
  }

  #[cfg(feature = "mmap")]
  pub(super) fn new_mmaped(cap: usize, file: std::fs::File, lock: bool) -> std::io::Result<Self> {
    use fs4::FileExt;

    unsafe {
      if lock {
        file.lock_exclusive()?;
      }

      file.set_len(cap as u64).and_then(|_| {
        memmapix::MmapOptions::new()
          .len(cap)
          .map_mut(&file)
          .map(|mmap| Self {
            cap,
            backend: SharedBackend::Mmap {
              buf: Box::into_raw(Box::new(mmap)),
              file,
              lock,
            },
            refs: AtomicUsize::new(1),
          })
      })
    }
  }

  #[cfg(feature = "mmap")]
  pub(super) fn new_mmaped_anon(cap: usize) -> std::io::Result<Self> {
    memmapix::MmapOptions::new()
      .len(cap)
      .map_anon()
      .map(|mmap| Self {
        cap,
        backend: SharedBackend::AnonymousMmap(mmap),
        refs: AtomicUsize::new(1),
      })
  }

  pub(super) fn as_mut_ptr(&mut self) -> *mut u8 {
    match &mut self.backend {
      SharedBackend::Vec(vec) => vec.as_mut_ptr(),
      #[cfg(feature = "mmap")]
      SharedBackend::Mmap { buf: mmap, .. } => unsafe { (**mmap).as_mut_ptr() },
      #[cfg(feature = "mmap")]
      SharedBackend::AnonymousMmap(mmap) => mmap.as_mut_ptr(),
    }
  }

  #[inline]
  pub(super) const fn cap(&self) -> usize {
    self.cap
  }

  /// Only works on mmap with a file backend, unmounts the memory mapped file and truncates it to the specified size.
  ///
  /// ## Safety:
  /// - This method must be invoked in the drop impl of `Arena`.
  pub(super) unsafe fn unmount(&mut self, _size: usize) {
    #[cfg(feature = "mmap")]
    if let SharedBackend::Mmap { buf, file, lock } = &self.backend {
      use fs4::FileExt;

      // Any errors during unmapping/closing are ignored as the only way
      // to report them would be through panicking which is highly discouraged
      // in Drop impls, c.f. https://github.com/rust-lang/lang-team/issues/97

      {
        let mmap = &**buf;
        let _ = mmap.flush();
      }

      // we must trigger the drop of the mmap before truncating the file
      drop(Box::from_raw(*buf));

      // relaxed ordering is enough here as we're in a drop, no one else can
      // access this memory anymore.
      let _ = file.set_len(_size as u64);
      if *lock {
        let _ = file.unlock();
      }
    }
  }
}
