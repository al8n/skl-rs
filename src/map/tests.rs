use super::*;
use crate::Descend;

use std::format;

use std::sync::Arc;

use rarena_allocator::Freelist;
#[cfg(feature = "std")]
use wg::WaitGroup;

const ARENA_SIZE: usize = 1 << 20;
#[cfg(feature = "std")]
const BIG_ARENA_SIZE: usize = 120 << 20;
const TEST_OPTIONS: Options = Options::new().with_capacity(ARENA_SIZE as u32);
const UNIFY_TEST_OPTIONS: Options = Options::new()
  .with_capacity(ARENA_SIZE as u32)
  .with_unify(true);
#[cfg(feature = "std")]
const BIG_TEST_OPTIONS: Options = Options::new().with_capacity(BIG_ARENA_SIZE as u32);
#[cfg(feature = "std")]
const UNIFY_BIG_TEST_OPTIONS: Options = Options::new()
  .with_capacity(BIG_ARENA_SIZE as u32)
  .with_unify(true);

fn run(f: impl Fn() + Send + Sync + 'static) {
  f();
}

/// Only used for testing

pub fn key(i: usize) -> std::vec::Vec<u8> {
  format!("{:05}", i).into_bytes()
}

/// Only used for testing
#[cfg(feature = "std")]
pub fn big_value(i: usize) -> std::vec::Vec<u8> {
  format!("{:01048576}", i).into_bytes()
}

/// Only used for testing
pub fn new_value(i: usize) -> std::vec::Vec<u8> {
  format!("{:05}", i).into_bytes()
}

fn make_int_key(i: usize) -> std::vec::Vec<u8> {
  format!("{:05}", i).into_bytes()
}

fn make_value(i: usize) -> std::vec::Vec<u8> {
  format!("v{:05}", i).into_bytes()
}

#[test]
fn test_node_ptr_clone() {
  let node_ptr = NodePtr::<u8>::NULL;
  #[allow(clippy::clone_on_copy)]
  let _ = node_ptr.clone();
}

#[test]
fn test_encode_decode_key_size() {
  // Test cases
  let test_cases = [
    (0, 0),                       // Minimum values
    (1, 1),                       // Small values
    (0x1FFFFFF, 0),               // Maximum key_size, minimum height
    (0, 0b11111),                 // Minimum key_size, maximum height
    (0x1FFFFFF, 0b11111),         // Maximum values
    (0x1FFFFFF - 1, 0b11111 - 1), // One less than maximum values
    (12345678, 31),               // Random values
    (0, 1),                       // Edge case: Minimum key_size, small height
    (1, 0),                       // Edge case: Small key_size, minimum height
  ];

  for &(key_size, height) in &test_cases {
    let encoded = encode_key_size_and_height(key_size, height);
    let (decoded_key_size, decoded_height) = decode_key_size_and_height(encoded);

    assert_eq!(key_size, decoded_key_size);
    assert_eq!(height, decoded_height);
  }
}

fn empty_in(l: SkipMap) {
  let mut it = l.iter_all_versions(0);

  assert!(it.seek_lower_bound(Bound::Unbounded).is_none());
  assert!(it.seek_upper_bound(Bound::Unbounded).is_none());
  assert!(it.seek_lower_bound(Bound::Included(b"aaa")).is_none());
  assert!(it.seek_upper_bound(Bound::Excluded(b"aaa")).is_none());
  assert!(it.seek_lower_bound(Bound::Excluded(b"aaa")).is_none());
  assert!(it.seek_upper_bound(Bound::Included(b"aaa")).is_none());
  assert!(l.first(0).is_none());
  assert!(l.last(0).is_none());
  assert!(l.ge(0, b"aaa").is_none());
  assert!(l.lt(0, b"aaa").is_none());
  assert!(l.gt(0, b"aaa").is_none());
  assert!(l.le(0, b"aaa").is_none());
  assert!(l.get(0, b"aaa").is_none());
  assert!(!l.contains_key(0, b"aaa"));
  assert!(l.allocated() > 0);
  assert!(l.capacity() > 0);
  assert_eq!(l.remaining(), l.capacity() - l.allocated());
}

#[test]
fn test_empty() {
  run(|| empty_in(SkipMap::new().unwrap()));
}

#[test]
fn test_empty_unify() {
  run(|| empty_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_empty_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_empty_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(1000))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();

    empty_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_empty_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(1000);
    empty_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_empty_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(1000);
    empty_in(SkipMap::map_anon_with_options(TEST_OPTIONS, map_options).unwrap());
  })
}

fn full_in(l: impl FnOnce(usize) -> SkipMap) {
  let l = l(1000);
  let mut found_arena_full = false;

  let mut full_at = 0;
  for i in 0..100 {
    if let Err(e) = l.get_or_insert(0, &make_int_key(i), &make_value(i)) {
      assert!(matches!(
        e,
        Error::Arena(ArenaError::InsufficientSpace { .. })
      ));
      found_arena_full = true;
      full_at = i;
      break;
    }
  }

  assert!(found_arena_full);

  let e = l
    .get_or_insert(0, &make_int_key(full_at + 1), &make_value(full_at + 1))
    .unwrap_err();

  assert!(matches!(
    e,
    Error::Arena(ArenaError::InsufficientSpace { .. })
  ));
}

#[test]
fn test_full() {
  run(|| {
    full_in(|n| {
      SkipMap::with_options(
        Options::new()
          .with_capacity(n as u32)
          .with_freelist(Freelist::None),
      )
      .unwrap()
    })
  })
}

#[test]
fn test_full_unify() {
  run(|| {
    full_in(|n| {
      SkipMap::with_options(
        UNIFY_TEST_OPTIONS
          .with_capacity(n as u32)
          .with_freelist(Freelist::None),
      )
      .unwrap()
    })
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_full_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_full_map_mut");

    full_in(|n| {
      let open_options = OpenOptions::default()
        .create_new(Some(n as u32))
        .read(true)
        .write(true);
      let map_options = MmapOptions::default();
      SkipMap::map_mut_with_options(
        p,
        Options::new().with_freelist(Freelist::None),
        open_options,
        map_options,
      )
      .unwrap()
    });
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_full_map_anon() {
  run(|| {
    full_in(|n| {
      let map_options = MmapOptions::default().len(n as u32);
      SkipMap::map_anon_with_options(Options::new().with_freelist(Freelist::None), map_options)
        .unwrap()
    });
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_full_map_anon_unify() {
  run(|| {
    full_in(|n| {
      let map_options = MmapOptions::default().len(n as u32);
      SkipMap::map_anon_with_options(Options::new().with_freelist(Freelist::None), map_options)
        .unwrap()
    });
  })
}

fn basic_in(mut l: SkipMap) {
  // Try adding values.
  l.get_or_insert(0, b"key1", &make_value(1)).unwrap();
  l.get_or_insert(0, b"key3", &make_value(3)).unwrap();
  l.get_or_insert(0, b"key2", &make_value(2)).unwrap();
  assert_eq!(l.comparator(), &Ascend);

  {
    let mut it = l.iter_all_versions(0);
    let ent = it.seek_lower_bound(Bound::Included(b"key1")).unwrap();
    assert_eq!(ent.key(), b"key1");
    assert_eq!(ent.value().unwrap(), &make_value(1));
    assert_eq!(ent.trailer().version(), 0);

    let ent = it.seek_lower_bound(Bound::Included(b"key2")).unwrap();
    assert_eq!(ent.key(), b"key2");
    assert_eq!(ent.value().unwrap(), &make_value(2));
    assert_eq!(ent.trailer().version(), 0);

    let ent = it.seek_lower_bound(Bound::Included(b"key3")).unwrap();
    assert_eq!(ent.key(), b"key3");
    assert_eq!(ent.value().unwrap(), &make_value(3));
    assert_eq!(ent.trailer().version(), 0);
  }

  l.get_or_insert(1, "a".as_bytes(), &[]).unwrap();
  l.get_or_insert(2, "a".as_bytes(), &[]).unwrap();

  {
    let mut it = l.iter_all_versions(2);
    let ent = it.seek_lower_bound(Bound::Included(b"a")).unwrap();
    assert_eq!(ent.key(), b"a");
    assert_eq!(ent.value().unwrap(), &[]);
    assert_eq!(ent.trailer().version(), 2);

    let ent = it.next().unwrap();
    assert_eq!(ent.key(), b"a");
    assert_eq!(ent.value().unwrap(), &[]);
    assert_eq!(ent.trailer().version(), 1);
  }

  l.get_or_insert(2, "b".as_bytes(), &[]).unwrap();
  l.get_or_insert(1, "b".as_bytes(), &[]).unwrap();

  {
    let mut it = l.iter_all_versions(2);
    let ent = it.seek_lower_bound(Bound::Included(b"b")).unwrap();
    assert_eq!(ent.key(), b"b");
    assert_eq!(ent.value().unwrap(), &[]);
    assert_eq!(ent.trailer().version(), 2);

    let ent = it.next().unwrap();
    assert_eq!(ent.key(), b"b");
    assert_eq!(ent.value().unwrap(), &[]);
    assert_eq!(ent.trailer().version(), 1);

    let ent = it.entry().unwrap();
    assert_eq!(ent.key(), b"b");
    assert_eq!(ent.value().unwrap(), &[]);
    assert_eq!(ent.trailer().version(), 1);
  }

  l.get_or_insert(2, b"b", &[]).unwrap().unwrap();

  assert!(l.get_or_insert(2, b"c", &[]).unwrap().is_none());

  {
    #[allow(clippy::clone_on_copy)]
    let a1 = l.get(1, b"a").unwrap().clone();
    let a2 = l.get(2, b"a").unwrap();
    assert!(a1 > a2);
    assert_ne!(a1, a2);
    let b1 = l.get(1, b"b").unwrap();
    let b2 = l.get(2, b"b").unwrap();
    assert!(b1 > b2);
    assert_ne!(b1, b2);
    let mut arr = [a1, a2, b1, b2];
    arr.sort();
    assert_eq!(arr, [a2, a1, b2, b1]);
  }

  unsafe {
    l.clear().unwrap();
  }

  let l = l.clone();
  {
    let mut it = l.iter_all_versions(0);
    assert!(it.seek_lower_bound(Bound::Unbounded).is_none());
    assert!(it.seek_upper_bound(Bound::Unbounded).is_none());
  }
  assert!(l.is_empty());

  #[cfg(feature = "memmap")]
  l.flush().unwrap();

  #[cfg(feature = "memmap")]
  l.flush_async().unwrap();
}

#[test]
fn test_basic() {
  run(|| basic_in(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_basic_unify() {
  run(|| basic_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_basic_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_basic_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    basic_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_basic_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    basic_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_basic_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    basic_in(SkipMap::map_anon_with_options(TEST_OPTIONS, map_options).unwrap());
  })
}

fn iter_all_versions_mvcc(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();

  let mut it = l.iter_all_versions(0);
  let mut num = 0;
  while it.next().is_some() {
    num += 1;
  }
  assert_eq!(num, 0);

  let mut it = l.iter_all_versions(1);
  let mut num = 0;
  while it.next().is_some() {
    num += 1;
  }
  assert_eq!(num, 2);

  let mut it = l.iter_all_versions(2);
  let mut num = 0;
  while it.next().is_some() {
    num += 1;
  }
  assert_eq!(num, 2);

  let mut it = l.iter_all_versions(3);
  let mut num = 0;
  while it.next().is_some() {
    num += 1;
  }
  assert_eq!(num, 4);

  let mut it = l.iter_all_versions(0);
  assert!(it.seek_lower_bound(Bound::Unbounded).is_none());
  assert!(it.seek_upper_bound(Bound::Unbounded).is_none());

  let mut it = l.iter_all_versions(1);
  let ent = it.seek_lower_bound(Bound::Unbounded).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value().unwrap(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = it.seek_upper_bound(Bound::Unbounded).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let mut it = l.iter_all_versions(2);
  let ent = it.seek_lower_bound(Bound::Unbounded).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value().unwrap(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = it.seek_upper_bound(Bound::Unbounded).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let mut it = l.iter_all_versions(3);
  let ent = it.min().unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value().unwrap(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = it.max().unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = it.seek_upper_bound(Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value().unwrap(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = it.seek_upper_bound(Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = it.seek_lower_bound(Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = it.seek_lower_bound(Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value().unwrap(), b"c2");
  assert_eq!(ent.trailer().version(), 3);
}

#[test]
fn test_iter_all_versions_mvcc() {
  run(|| iter_all_versions_mvcc(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_iter_all_versions_mvcc_unify() {
  run(|| iter_all_versions_mvcc(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_all_versions_mvcc_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_iter_all_versions_mvcc_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_mvcc(SkipMap::map_mut(p, open_options, map_options).unwrap());
  });
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_mvcc_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_mvcc(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_mvcc_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_mvcc(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn ordering() {
  let l = SkipMap::with_options_and_comparator(TEST_OPTIONS, Descend).unwrap();

  l.get_or_insert(1, b"a1", b"a1").unwrap();
  l.get_or_insert(2, b"a2", b"a2").unwrap();
  l.get_or_insert(3, b"a3", b"a3").unwrap();

  let mut it = l.iter_all_versions(3);
  for i in (1..=3).rev() {
    let ent = it.next().unwrap();
    assert_eq!(ent.key(), format!("a{i}").as_bytes());
    assert_eq!(ent.value().unwrap(), format!("a{i}").as_bytes());
  }
}

#[test]
fn test_ordering() {
  run(ordering);
}

fn get_mvcc(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();

  let ent = l.get(1, b"a").unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.get(2, b"a").unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.get(3, b"a").unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.get(4, b"a").unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  assert!(l.get(0, b"b").is_none());
  assert!(l.get(1, b"b").is_none());
  assert!(l.get(2, b"b").is_none());
  assert!(l.get(3, b"b").is_none());
  assert!(l.get(4, b"b").is_none());

  let ent = l.get(1, b"c").unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.get(2, b"c").unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.get(3, b"c").unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.get(4, b"c").unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  assert!(l.get(5, b"d").is_none());
}

#[test]
fn test_get_mvcc() {
  run(|| get_mvcc(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_get_mvcc_unify() {
  run(|| get_mvcc(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_get_mvcc_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_get_mvcc_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    get_mvcc(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_mvcc_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_mvcc(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_mvcc_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_mvcc(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn gt_in(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();
  l.get_or_insert(5, b"c", b"c3").unwrap();

  assert!(l.lower_bound(0, Bound::Excluded(b"a")).is_none());
  assert!(l.lower_bound(0, Bound::Excluded(b"b")).is_none());
  assert!(l.lower_bound(0, Bound::Excluded(b"c")).is_none());

  let ent = l.lower_bound(1, Bound::Excluded(b"")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Excluded(b"")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Excluded(b"")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(1, Bound::Excluded(b"a")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Excluded(b"a")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Excluded(b"a")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(1, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(4, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(5, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c3");
  assert_eq!(ent.trailer().version(), 5);

  let ent = l.lower_bound(6, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c3");
  assert_eq!(ent.trailer().version(), 5);

  assert!(l.lower_bound(1, Bound::Excluded(b"c")).is_none());
  assert!(l.lower_bound(2, Bound::Excluded(b"c")).is_none());
  assert!(l.lower_bound(3, Bound::Excluded(b"c")).is_none());
  assert!(l.lower_bound(4, Bound::Excluded(b"c")).is_none());
  assert!(l.lower_bound(5, Bound::Excluded(b"c")).is_none());
  assert!(l.lower_bound(6, Bound::Excluded(b"c")).is_none());
}

#[test]
fn test_gt() {
  run(|| gt_in(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_gt_unify() {
  run(|| gt_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_gt_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_gt_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    gt_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_gt_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    gt_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_gt_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    gt_in(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn ge_in(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();

  assert!(l.lower_bound(0, Bound::Included(b"a")).is_none());
  assert!(l.lower_bound(0, Bound::Included(b"b")).is_none());
  assert!(l.lower_bound(0, Bound::Included(b"c")).is_none());

  let ent = l.lower_bound(1, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(4, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(1, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(4, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(1, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(2, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.lower_bound(3, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.lower_bound(4, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  assert!(l.lower_bound(0, Bound::Included(b"d")).is_none());
  assert!(l.lower_bound(1, Bound::Included(b"d")).is_none());
  assert!(l.lower_bound(2, Bound::Included(b"d")).is_none());
  assert!(l.lower_bound(3, Bound::Included(b"d")).is_none());
  assert!(l.lower_bound(4, Bound::Included(b"d")).is_none());
}

#[test]
fn test_ge() {
  run(|| ge_in(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_ge_unify() {
  run(|| ge_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_ge_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_ge_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    ge_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_ge_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    ge_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_ge_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    ge_in(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn le_in(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();

  assert!(l.upper_bound(0, Bound::Included(b"a")).is_none());
  assert!(l.upper_bound(0, Bound::Included(b"b")).is_none());
  assert!(l.upper_bound(0, Bound::Included(b"c")).is_none());

  let ent = l.upper_bound(1, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Included(b"a")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(1, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Included(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(1, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Included(b"c")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(1, Bound::Included(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Included(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Included(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Included(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);
}

#[test]
fn test_le() {
  run(|| le_in(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_le_unify() {
  run(|| le_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_le_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_le_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    gt_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_le_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    gt_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_le_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    gt_in(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn lt_in(l: SkipMap) {
  l.get_or_insert(1, b"a", b"a1").unwrap();
  l.get_or_insert(3, b"a", b"a2").unwrap();
  l.get_or_insert(1, b"c", b"c1").unwrap();
  l.get_or_insert(3, b"c", b"c2").unwrap();

  assert!(l.upper_bound(0, Bound::Excluded(b"a")).is_none());
  assert!(l.upper_bound(0, Bound::Excluded(b"b")).is_none());
  assert!(l.upper_bound(0, Bound::Excluded(b"c")).is_none());
  assert!(l.upper_bound(1, Bound::Excluded(b"a")).is_none());
  assert!(l.upper_bound(2, Bound::Excluded(b"a")).is_none());

  let ent = l.upper_bound(1, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Excluded(b"b")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(1, Bound::Excluded(b"c")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Excluded(b"c")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Excluded(b"c")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Excluded(b"c")).unwrap();
  assert_eq!(ent.key(), b"a");
  assert_eq!(ent.value(), b"a2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(1, Bound::Excluded(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(2, Bound::Excluded(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c1");
  assert_eq!(ent.trailer().version(), 1);

  let ent = l.upper_bound(3, Bound::Excluded(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);

  let ent = l.upper_bound(4, Bound::Excluded(b"d")).unwrap();
  assert_eq!(ent.key(), b"c");
  assert_eq!(ent.value(), b"c2");
  assert_eq!(ent.trailer().version(), 3);
}

#[test]
fn test_lt() {
  run(|| lt_in(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_lt_unify() {
  run(|| lt_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_lt_map_mut() {
  let dir = tempfile::tempdir().unwrap();
  let p = dir.path().join("test_skipmap_lt_map_mut");
  let open_options = OpenOptions::default()
    .create_new(Some(ARENA_SIZE as u32))
    .read(true)
    .write(true);
  let map_options = MmapOptions::default();
  lt_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
}

#[test]
#[cfg(feature = "memmap")]

fn test_lt_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    lt_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_lt_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    lt_in(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn test_basic_large_testcases_in(l: Arc<SkipMap>) {
  let n = 1000;

  for i in 0..n {
    l.get_or_insert(0, &key(i), &new_value(i)).unwrap();
  }

  for i in 0..n {
    let k = key(i);
    let ent = l.get(0, &k).unwrap();
    assert_eq!(new_value(i), ent.value());
    assert_eq!(ent.trailer().version(), 0);
    assert_eq!(ent.key(), k);
  }

  assert_eq!(n, l.len());
}

#[test]
fn test_basic_large_testcases() {
  run(|| {
    let l = Arc::new(SkipMap::with_options(TEST_OPTIONS).unwrap());
    test_basic_large_testcases_in(l);
  })
}

#[test]
fn test_basic_large_testcases_unify() {
  run(|| {
    let l = Arc::new(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap());
    test_basic_large_testcases_in(l);
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_basic_large_testcases_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_basic_large_testcases_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    let l = Arc::new(SkipMap::map_mut(p, open_options, map_options).unwrap());
    test_basic_large_testcases_in(l);
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_basic_large_testcases_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    let l = Arc::new(SkipMap::map_anon(map_options).unwrap());
    test_basic_large_testcases_in(l);
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_basic_large_testcases_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    let l = Arc::new(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
    test_basic_large_testcases_in(l);
  })
}

#[cfg(feature = "std")]
fn test_concurrent_basic_runner(l: Arc<SkipMap>) {
  #[cfg(not(any(miri, feature = "loom")))]
  const N: usize = 1000;
  #[cfg(any(miri, feature = "loom"))]
  const N: usize = 5;

  let wg = Arc::new(());
  for i in 0..N {
    let w = wg.clone();
    let l = l.clone();
    std::thread::spawn(move || {
      l.get_or_insert(0, &key(i), &new_value(i)).unwrap();
      drop(w);
    });
  }
  while Arc::strong_count(&wg) > 1 {}
  for i in 0..N {
    let w = wg.clone();
    let l = l.clone();
    std::thread::spawn(move || {
      let k = key(i);
      assert_eq!(l.get(0, &k).unwrap().value(), new_value(i), "broken: {i}");
      drop(w);
    });
  }
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_basic() {
  run(|| {
    let l = Arc::new(
      SkipMap::with_options(TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    );
    test_concurrent_basic_runner(l);
  })
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_basic_unify() {
  run(|| {
    let l = Arc::new(
      SkipMap::with_options(UNIFY_TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    );
    test_concurrent_basic_runner(l);
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_concurrent_basic_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    let l = Arc::new(
      SkipMap::map_mut(p, open_options, map_options)
        .unwrap()
        .with_yield_now(),
    );
    test_concurrent_basic_runner(l);
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_basic_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    test_concurrent_basic_runner(Arc::new(
      SkipMap::map_anon(map_options).unwrap().with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_basic_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    test_concurrent_basic_runner(Arc::new(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[cfg(feature = "std")]
fn test_concurrent_basic_big_values_runner(l: Arc<SkipMap>) {
  #[cfg(not(any(miri, feature = "loom")))]
  const N: usize = 100;
  #[cfg(any(miri, feature = "loom"))]
  const N: usize = 5;

  for i in 0..N {
    let l = l.clone();
    std::thread::spawn(move || {
      l.get_or_insert(0, &key(i), &big_value(i)).unwrap();
    });
  }
  while Arc::strong_count(&l) > 1 {}
  // assert_eq!(N, l.len());
  for i in 0..N {
    let l = l.clone();
    std::thread::spawn(move || {
      let k = key(i);
      assert_eq!(l.get(0, &k).unwrap().value(), big_value(i), "broken: {i}");
    });
  }
  while Arc::strong_count(&l) > 1 {}
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_basic_big_values() {
  run(|| {
    test_concurrent_basic_big_values_runner(Arc::new(
      SkipMap::with_options(BIG_TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_basic_big_values_unify() {
  run(|| {
    test_concurrent_basic_big_values_runner(Arc::new(
      SkipMap::with_options(UNIFY_BIG_TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_big_values_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_concurrent_basic_big_values_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(120 << 20))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    test_concurrent_basic_big_values_runner(Arc::new(
      SkipMap::map_mut(p, open_options, map_options)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_basic_big_values_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(120 << 20);
    test_concurrent_basic_big_values_runner(Arc::new(
      SkipMap::map_anon(map_options).unwrap().with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_basic_big_values_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(120 << 20);
    test_concurrent_basic_big_values_runner(Arc::new(
      SkipMap::map_anon_with_options(UNIFY_BIG_TEST_OPTIONS, map_options)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[cfg(feature = "std")]
fn concurrent_one_key(l: Arc<SkipMap>) {
  #[cfg(not(any(miri, feature = "loom")))]
  const N: usize = 100;
  #[cfg(any(miri, feature = "loom"))]
  const N: usize = 5;

  let wg = WaitGroup::new();
  for i in 0..N {
    let wg = wg.add(1);
    let l = l.clone();
    std::thread::spawn(move || {
      let _ = l.get_or_insert(0, b"thekey", &make_value(i));
      wg.done();
    });
  }

  wg.wait();

  let saw_value = Arc::new(crate::sync::AtomicU32::new(0));
  for _ in 0..N {
    let wg = wg.add(1);
    let l = l.clone();
    let saw_value = saw_value.clone();
    std::thread::spawn(move || {
      let ent = l.get(0, b"thekey").unwrap();
      let val = ent.value();
      let num: usize = core::str::from_utf8(&val[1..]).unwrap().parse().unwrap();
      assert!((0..N).contains(&num));

      let mut it = l.iter_all_versions(0);
      let ent = it.seek_lower_bound(Bound::Included(b"thekey")).unwrap();
      let val = ent.value().unwrap();
      let num: usize = core::str::from_utf8(&val[1..]).unwrap().parse().unwrap();
      assert!((0..N).contains(&num));
      assert_eq!(ent.key(), b"thekey");
      saw_value.fetch_add(1, Ordering::SeqCst);
      wg.done();
    });
  }

  wg.wait();

  assert_eq!(N, saw_value.load(Ordering::SeqCst) as usize);
  assert_eq!(l.len(), 1);
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_one_key() {
  run(|| {
    concurrent_one_key(Arc::new(
      SkipMap::with_options(TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "std")]
fn test_concurrent_one_key_unify() {
  run(|| {
    concurrent_one_key(Arc::new(
      SkipMap::with_options(UNIFY_TEST_OPTIONS)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_concurrent_one_key_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_concurrent_one_key_map_mut");
    let open_options = OpenOptions::default()
      .create(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    concurrent_one_key(Arc::new(
      SkipMap::map_mut(p, open_options, map_options)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_one_key_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    concurrent_one_key(Arc::new(
      SkipMap::map_anon(map_options).unwrap().with_yield_now(),
    ));
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_concurrent_one_key_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    concurrent_one_key(Arc::new(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options)
        .unwrap()
        .with_yield_now(),
    ));
  })
}

fn iter_all_versions_next(l: SkipMap) {
  const N: usize = 100;

  for i in (0..N).rev() {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  let mut it = l.iter_all_versions(0);
  let mut ent = it.seek_lower_bound(Bound::Unbounded).unwrap();
  for i in 0..N {
    assert_eq!(ent.key(), make_int_key(i));
    assert_eq!(ent.value().unwrap(), make_value(i));
    if i != N - 1 {
      ent = it.next().unwrap();
    }
  }

  assert!(it.next().is_none());
}

#[test]
fn test_iter_all_versions_next() {
  run(|| iter_all_versions_next(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_iter_all_versions_next_unify() {
  run(|| iter_all_versions_next(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_all_versions_next_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_iter_all_versions_next_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_next(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_next_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_next(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_next_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_next(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn range_next(l: SkipMap) {
  const N: usize = 100;

  for i in (0..N).rev() {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  let upper = make_int_key(50);
  let mut it = l.range(0, ..=upper.as_slice());
  let mut ent = it.seek_lower_bound(Bound::Unbounded);
  for i in 0..N {
    if i <= 50 {
      {
        let ent = ent.unwrap();
        assert_eq!(ent.key(), make_int_key(i));
        assert_eq!(ent.value(), make_value(i));
      }
      ent = it.next();
    } else {
      assert!(ent.is_none());
      ent = it.next();
    }
  }

  assert!(it.next().is_none());
}

#[test]
fn test_range_next() {
  run(|| range_next(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_range_next_unify() {
  run(|| range_next(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_range_next_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_range_next_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_next(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_next_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_next(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_next_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_next(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn iter_all_versions_prev(l: SkipMap) {
  const N: usize = 100;

  for i in 0..N {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  let mut it = l.iter_all_versions(0);
  let mut ent = it.seek_upper_bound(Bound::Unbounded).unwrap();
  for i in (0..N).rev() {
    assert_eq!(ent.key(), make_int_key(i));
    assert_eq!(ent.value().unwrap(), make_value(i));
    if i != 0 {
      ent = it.next_back().unwrap();
    }
  }

  assert!(it.next_back().is_none());
}

#[test]
fn test_iter_all_versions_next_back() {
  run(|| iter_all_versions_prev(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_all_versions_prev_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_iter_all_versions_prev_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_prev(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_prev_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_prev(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_prev_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_prev(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn range_prev(l: SkipMap) {
  const N: usize = 100;

  for i in 0..N {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  let lower = make_int_key(50);
  let mut it = l.range(0, lower.as_slice()..);
  let mut ent = it.seek_upper_bound(Bound::Unbounded);
  for i in (0..N).rev() {
    if i >= 50 {
      {
        let ent = ent.unwrap();
        assert_eq!(ent.key(), make_int_key(i));
        assert_eq!(ent.value(), make_value(i));
      }
      ent = it.next_back();
    } else {
      assert!(ent.is_none());
      ent = it.next_back();
    }
  }

  assert!(it.next_back().is_none());
}

#[test]
fn test_range_prev() {
  run(|| range_prev(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_range_prev_unify() {
  run(|| range_prev(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_range_prev_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_range_prev_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    range_prev(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_prev_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range_prev(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_prev_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range_prev(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn iter_all_versions_seek_ge(l: SkipMap) {
  const N: usize = 100;

  for i in (0..N).rev() {
    let v = i * 10 + 1000;
    l.get_or_insert(0, &make_int_key(v), &make_value(v))
      .unwrap();
  }

  let mut it = l.iter_all_versions(0);
  let ent = it.seek_lower_bound(Bound::Included(b"")).unwrap();
  assert_eq!(ent.key(), make_int_key(1000));
  assert_eq!(ent.value().unwrap(), make_value(1000));

  let ent = it.seek_lower_bound(Bound::Included(b"01000")).unwrap();
  assert_eq!(ent.key(), make_int_key(1000));
  assert_eq!(ent.value().unwrap(), make_value(1000));

  let ent = it.seek_lower_bound(Bound::Included(b"01005")).unwrap();
  assert_eq!(ent.key(), make_int_key(1010));
  assert_eq!(ent.value().unwrap(), make_value(1010));

  let ent = it.seek_lower_bound(Bound::Included(b"01010")).unwrap();
  assert_eq!(ent.key(), make_int_key(1010));
  assert_eq!(ent.value().unwrap(), make_value(1010));

  let ent = it.seek_lower_bound(Bound::Included(b"01020")).unwrap();
  assert_eq!(ent.key(), make_int_key(1020));
  assert_eq!(ent.value().unwrap(), make_value(1020));

  let ent = it.seek_lower_bound(Bound::Included(b"01200")).unwrap();
  assert_eq!(ent.key(), make_int_key(1200));
  assert_eq!(ent.value().unwrap(), make_value(1200));

  let ent = it.seek_lower_bound(Bound::Included(b"01100")).unwrap();
  assert_eq!(ent.key(), make_int_key(1100));
  assert_eq!(ent.value().unwrap(), make_value(1100));

  let ent = it.seek_lower_bound(Bound::Included(b"99999"));
  assert!(ent.is_none());

  l.get_or_insert(0, &[], &[]).unwrap();
  let ent = it.seek_lower_bound(Bound::Included(b"")).unwrap();
  assert_eq!(ent.key(), &[]);
  assert_eq!(ent.value().unwrap(), &[]);

  let ent = it.seek_lower_bound(Bound::Included(b"")).unwrap();
  assert_eq!(ent.key(), &[]);
  assert_eq!(ent.value().unwrap(), &[]);
}

#[test]
fn test_iter_all_versions_seek_ge() {
  run(|| iter_all_versions_seek_ge(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_iter_all_versions_seek_ge_unify() {
  run(|| iter_all_versions_seek_ge(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_all_versions_seek_ge_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_iter_all_versions_seek_ge_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_seek_ge(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_seek_ge_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_seek_ge(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_seek_ge_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_seek_ge(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn iter_all_versions_seek_lt(l: SkipMap) {
  const N: usize = 100;

  for i in (0..N).rev() {
    let v = i * 10 + 1000;
    l.get_or_insert(0, &make_int_key(v), &make_value(v))
      .unwrap();
  }

  let mut it = l.iter_all_versions(0);
  assert!(it.seek_upper_bound(Bound::Excluded(b"")).is_none());

  let ent = it.seek_upper_bound(Bound::Excluded(b"01000"));
  assert!(ent.is_none());

  let ent = it.seek_upper_bound(Bound::Excluded(b"01001")).unwrap();
  assert_eq!(ent.key(), make_int_key(1000));
  assert_eq!(ent.value().unwrap(), make_value(1000));

  let ent = it.seek_upper_bound(Bound::Excluded(b"01991")).unwrap();
  assert_eq!(ent.key(), make_int_key(1990));
  assert_eq!(ent.value().unwrap(), make_value(1990));

  let ent = it.seek_upper_bound(Bound::Excluded(b"99999")).unwrap();
  assert_eq!(ent.key(), make_int_key(1990));
  assert_eq!(ent.value().unwrap(), make_value(1990));

  l.get_or_insert(0, &[], &[]).unwrap();
  assert!(l.lt(0, &[]).is_none());

  let ent = it.seek_upper_bound(Bound::Excluded(b""));
  assert!(ent.is_none());

  let ent = it.seek_upper_bound(Bound::Excluded(b"\x01")).unwrap();
  assert_eq!(ent.key(), &[]);
  assert_eq!(ent.value().unwrap(), &[]);
}

#[test]
fn test_iter_all_versions_seek_lt() {
  run(|| iter_all_versions_seek_lt(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_iter_all_versions_seek_lt_unify() {
  run(|| iter_all_versions_seek_lt(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_all_versions_seek_lt_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_iter_all_versions_seek_lt_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_all_versions_seek_lt(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_seek_lt_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_seek_lt(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_all_versions_seek_lt_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_all_versions_seek_lt(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn range(l: SkipMap) {
  for i in 1..10 {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  let k3 = make_int_key(3);
  let k7 = make_int_key(7);
  let mut it = l.range(0, k3.as_slice()..k7.as_slice()).clone();
  assert_eq!(it.bounds(), &(k3.as_slice()..k7.as_slice()));

  for i in 3..=6 {
    let k = make_int_key(i);
    let ent = it.seek_lower_bound(Bound::Included(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(i));
    assert_eq!(ent.value(), make_value(i));
  }

  for i in 1..3 {
    let k = make_int_key(i);
    let ent = it.seek_lower_bound(Bound::Included(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(3));
    assert_eq!(ent.value(), make_value(3));
  }

  for i in 7..10 {
    let k = make_int_key(i);
    assert!(it.seek_lower_bound(Bound::Included(&k)).is_none());
  }

  for i in 7..10 {
    let k = make_int_key(i);
    let ent = it.seek_upper_bound(Bound::Included(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(6));
    assert_eq!(ent.value(), make_value(6));
  }

  let ent = it
    .seek_lower_bound(Bound::Included(&make_int_key(6)))
    .unwrap();
  assert_eq!(ent.key(), make_int_key(6));
  assert_eq!(ent.value(), make_value(6));

  assert!(it.next().is_none());

  let ent = it
    .seek_upper_bound(Bound::Included(&make_int_key(6)))
    .unwrap();
  assert_eq!(ent.key(), make_int_key(6));
  assert_eq!(ent.value(), make_value(6));

  assert!(it.next().is_none());

  for i in 4..=7 {
    let k = make_int_key(i);
    let ent = it.seek_upper_bound(Bound::Excluded(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(i - 1));
    assert_eq!(ent.value(), make_value(i - 1));
  }

  for i in 7..10 {
    let k = make_int_key(i);
    let ent = it.seek_upper_bound(Bound::Excluded(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(6));
    assert_eq!(ent.value(), make_value(6));
  }

  for i in 1..3 {
    let k = make_int_key(i);
    let ent = it.seek_lower_bound(Bound::Excluded(&k)).unwrap();
    assert_eq!(ent.key(), make_int_key(3));
    assert_eq!(ent.value(), make_value(3));
  }

  for i in 1..4 {
    let k = make_int_key(i);
    assert!(it.seek_upper_bound(Bound::Excluded(&k)).is_none());
  }

  let ent = it
    .seek_upper_bound(Bound::Excluded(&make_int_key(4)))
    .unwrap();
  assert_eq!(ent.key(), make_int_key(3));
  assert_eq!(ent.value(), make_value(3));

  assert!(it.next_back().is_none());
}

#[test]
fn test_range() {
  run(|| range(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_range_unify() {
  run(|| range(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_range_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_range_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    range(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn iter_latest(l: SkipMap) {
  const N: usize = 100;

  for i in 0..N {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  for i in 50..N {
    l.get_or_insert(1, &make_int_key(i), &make_value(i + 1000))
      .unwrap();
  }

  for i in 0..50 {
    l.get_or_insert(2, &make_int_key(i), &make_value(i + 1000))
      .unwrap();
  }

  let mut it = l.iter(4);
  let mut num = 0;
  for i in 0..N {
    let ent = it.next().unwrap();
    assert_eq!(ent.key(), make_int_key(i));
    assert_eq!(ent.value(), make_value(i + 1000));

    num += 1;
  }
  assert_eq!(num, N);
}

#[test]
fn test_iter_latest() {
  run(|| iter_latest(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_iter_latest_unify() {
  run(|| iter_latest(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_iter_latest_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_iter_latest_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    iter_latest(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_latest_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_latest(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_iter_latest_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    iter_latest(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn range_latest(l: SkipMap) {
  const N: usize = 100;

  for i in 0..N {
    l.get_or_insert(0, &make_int_key(i), &make_value(i))
      .unwrap();
  }

  for i in 50..N {
    l.get_or_insert(1, &make_int_key(i), &make_value(i + 1000))
      .unwrap();
  }

  for i in 0..50 {
    l.get_or_insert(2, &make_int_key(i), &make_value(i + 1000))
      .unwrap();
  }

  let mut it = l.range(4, ..);
  let mut num = 0;
  for i in 0..N {
    let ent = it.next().unwrap();
    assert_eq!(ent.key(), make_int_key(i));
    assert_eq!(ent.value(), make_value(i + 1000));

    num += 1;
  }
  assert_eq!(num, N);
}

#[test]
fn test_range_latest() {
  run(|| range_latest(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_range_latest_unify() {
  run(|| range_latest(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_range_latest_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_range_latest_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    range_latest(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_latest_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range_latest(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_range_latest_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    range_latest(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_reopen_mmap() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("reopen_skipmap");
    {
      let open_options = OpenOptions::default()
        .create(Some(ARENA_SIZE as u32))
        .read(true)
        .write(true);
      let map_options = MmapOptions::default();
      let l = SkipMap::map_mut(&p, open_options, map_options).unwrap();
      for i in 0..1000 {
        l.get_or_insert(0, &key(i), &new_value(i)).unwrap();
      }
      l.flush().unwrap();
    }

    let open_options = OpenOptions::default().read(true);
    let map_options = MmapOptions::default();
    let l = SkipMap::<u64>::map(&p, open_options, map_options, 0).unwrap();
    assert_eq!(1000, l.len());
    for i in 0..1000 {
      let k = key(i);
      let ent = l.get(0, &k).unwrap();
      assert_eq!(new_value(i), ent.value());
      assert_eq!(ent.trailer().version(), 0);
      assert_eq!(ent.key(), k);
    }
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_reopen_mmap2() {
  run(|| {
    use rand::seq::SliceRandom;

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("reopen_skipmap2");
    {
      let open_options = OpenOptions::default()
        .create(Some(ARENA_SIZE as u32))
        .read(true)
        .write(true);
      let map_options = MmapOptions::default();
      let l = SkipMap::map_mut_with_comparator(&p, open_options, map_options, Ascend).unwrap();
      let mut data = (0..1000).collect::<Vec<usize>>();
      data.shuffle(&mut rand::thread_rng());
      for i in data {
        l.get_or_insert(i as u64, &key(i), &new_value(i)).unwrap();
      }
      l.flush_async().unwrap();
      assert_eq!(l.max_version(), 999);
      assert_eq!(l.min_version(), 0);
    }

    let open_options = OpenOptions::default().read(true);
    let map_options = MmapOptions::default();
    let l = SkipMap::<u64, Ascend>::map_with_comparator(&p, open_options, map_options, Ascend, 0)
      .unwrap();
    assert_eq!(1000, l.len());
    let mut data = (0..1000).collect::<Vec<usize>>();
    data.shuffle(&mut rand::thread_rng());
    for i in data {
      let k = key(i);
      let ent = l.get(i as u64, &k).unwrap();
      assert_eq!(new_value(i), ent.value());
      assert_eq!(ent.trailer().version(), i as u64);
      assert_eq!(ent.key(), k);
    }
    assert_eq!(l.max_version(), 999);
    assert_eq!(l.min_version(), 0);
  })
}

struct Person {
  id: u32,
  name: std::string::String,
}

impl Person {
  fn encoded_size(&self) -> usize {
    4 + self.name.len()
  }
}

fn get_or_insert_with_value(l: SkipMap) {
  let alice = Person {
    id: 1,
    name: std::string::String::from("Alice"),
  };

  let encoded_size = alice.encoded_size() as u32;

  l.get_or_insert_with_value::<()>(1, b"alice", encoded_size, |val| {
    assert_eq!(val.capacity(), encoded_size as usize);
    assert!(val.is_empty());
    val.write(&alice.id.to_le_bytes()).unwrap();
    assert_eq!(val.len(), 4);
    assert_eq!(val.remaining(), encoded_size as usize - 4);
    assert_eq!(&*val, alice.id.to_le_bytes());
    val[..4].copy_from_slice(&alice.id.to_be_bytes());
    assert_eq!(&*val, alice.id.to_be_bytes());
    val.write(alice.name.as_bytes()).unwrap();
    assert_eq!(val.len(), encoded_size as usize);
    let err = val.write(&[1]).unwrap_err();
    assert_eq!(
      std::string::ToString::to_string(&err),
      "buffer does not have enough space (remaining 0, want 1)"
    );
    Ok(())
  })
  .unwrap();
}

#[test]
fn test_get_or_insert_with_value() {
  run(|| {
    get_or_insert_with_value(SkipMap::with_options(TEST_OPTIONS).unwrap());
  })
}

#[test]
fn test_get_or_insert_with_value_unify() {
  run(|| {
    get_or_insert_with_value(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_get_or_insert_with_value_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_get_or_insert_with_value_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    get_or_insert_with_value(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_insert_with_value_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_insert_with_value(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_insert_with_value_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_insert_with_value(
      SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap(),
    );
  })
}

fn get_or_insert_with(l: SkipMap) {
  let alice = Person {
    id: 1,
    name: std::string::String::from("Alice"),
  };

  let encoded_size = alice.encoded_size() as u32;

  l.get_or_insert_with::<()>(
    1,
    u27::new(5),
    |key| {
      key.write(b"alice").unwrap();
      Ok(())
    },
    encoded_size,
    |val| {
      assert_eq!(val.capacity(), encoded_size as usize);
      assert!(val.is_empty());
      val.write(&alice.id.to_le_bytes()).unwrap();
      assert_eq!(val.len(), 4);
      assert_eq!(val.remaining(), encoded_size as usize - 4);
      assert_eq!(&*val, alice.id.to_le_bytes());
      val[..4].copy_from_slice(&alice.id.to_be_bytes());
      assert_eq!(&*val, alice.id.to_be_bytes());
      val.write(alice.name.as_bytes()).unwrap();
      assert_eq!(val.len(), encoded_size as usize);
      let err = val.write(&[1]).unwrap_err();
      assert_eq!(
        std::string::ToString::to_string(&err),
        "buffer does not have enough space (remaining 0, want 1)"
      );
      Ok(())
    },
  )
  .unwrap();
}

#[test]
fn test_get_or_insert_with() {
  run(|| get_or_insert_with(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_get_or_insert_with_unify() {
  run(|| get_or_insert_with(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_get_or_insert_with_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_get_or_insert_with_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    get_or_insert_with(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_insert_with_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_insert_with(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_insert_with_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_insert_with(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn insert_in(l: SkipMap) {
  let k = 0u64.to_le_bytes();
  for i in 0..100 {
    let v = new_value(i);
    let old = l.insert(0, &k, &v).unwrap();
    if let Some(old) = old {
      assert_eq!(old.key(), k);
      assert_eq!(old.value(), new_value(i - 1));
    }
  }

  let ent = l.get(0, &k).unwrap();
  assert_eq!(ent.key(), k);
  assert_eq!(ent.value(), new_value(99));
}

#[test]
fn test_insert_in() {
  run(|| {
    insert_in(SkipMap::with_options(TEST_OPTIONS).unwrap());
  })
}

#[test]
fn test_insert_in_unify() {
  run(|| {
    insert_in(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_insert_in_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_insert_in_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    insert_in(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_in_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_in(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_in_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_in(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn insert_with_value(l: SkipMap) {
  let alice = Person {
    id: 1,
    name: std::string::String::from("Alice"),
  };

  let encoded_size = alice.encoded_size() as u32;

  l.insert_with_value::<()>(1, b"alice", encoded_size, |val| {
    assert_eq!(val.capacity(), encoded_size as usize);
    assert!(val.is_empty());
    val.write(&alice.id.to_le_bytes()).unwrap();
    assert_eq!(val.len(), 4);
    assert_eq!(val.remaining(), encoded_size as usize - 4);
    assert_eq!(val, alice.id.to_le_bytes());
    val[..4].copy_from_slice(&alice.id.to_be_bytes());
    assert_eq!(val, alice.id.to_be_bytes());
    val.write(alice.name.as_bytes()).unwrap();
    assert_eq!(val.len(), encoded_size as usize);
    let err = val.write(&[1]).unwrap_err();
    assert_eq!(
      std::string::ToString::to_string(&err),
      "buffer does not have enough space (remaining 0, want 1)"
    );
    Ok(())
  })
  .unwrap();

  let alice2 = Person {
    id: 2,
    name: std::string::String::from("Alice"),
  };

  let old = l
    .insert_with_value::<()>(1, b"alice", encoded_size, |val| {
      assert_eq!(val.capacity(), encoded_size as usize);
      assert!(val.is_empty());
      val.write(&alice2.id.to_le_bytes()).unwrap();
      assert_eq!(val.len(), 4);
      assert_eq!(val.remaining(), encoded_size as usize - 4);
      assert_eq!(&*val, alice2.id.to_le_bytes());
      val[..4].copy_from_slice(&alice2.id.to_be_bytes());
      assert_eq!(&*val, alice2.id.to_be_bytes());
      val.write(alice2.name.as_bytes()).unwrap();
      assert_eq!(val.len(), encoded_size as usize);
      let err = val.write(&[1]).unwrap_err();
      assert_eq!(
        std::string::ToString::to_string(&err),
        "buffer does not have enough space (remaining 0, want 1)"
      );
      Ok(())
    })
    .unwrap()
    .unwrap();

  assert_eq!(old.key(), b"alice");
  assert!(old.value().starts_with(&alice.id.to_be_bytes()));

  let ent = l.get(1, b"alice").unwrap();
  assert_eq!(ent.key(), b"alice");
  assert!(ent.value().starts_with(&alice2.id.to_be_bytes()));
}

#[test]
fn test_insert_with_value() {
  run(|| insert_with_value(SkipMap::with_options(TEST_OPTIONS).unwrap()));
}

#[test]
fn test_insert_with_value_unify() {
  run(|| insert_with_value(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()));
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_insert_with_value_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir
      .path()
      .join("test_skipmap_get_or_insert_with_value_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    insert_with_value(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_with_value_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_with_value(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_with_value_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_with_value(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn insert_with(l: SkipMap) {
  let alice = Person {
    id: 1,
    name: std::string::String::from("Alice"),
  };

  let encoded_size = alice.encoded_size() as u32;

  l.insert_with::<()>(
    1,
    u27::new(5),
    |key| {
      key.write(b"alice").unwrap();
      Ok(())
    },
    encoded_size,
    |val| {
      assert_eq!(val.capacity(), encoded_size as usize);
      assert!(val.is_empty());
      val.write(&alice.id.to_le_bytes()).unwrap();
      assert_eq!(val.len(), 4);
      assert_eq!(val.remaining(), encoded_size as usize - 4);
      assert_eq!(val, alice.id.to_le_bytes());
      val[..4].copy_from_slice(&alice.id.to_be_bytes());
      assert_eq!(val, alice.id.to_be_bytes());
      val.write(alice.name.as_bytes()).unwrap();
      assert_eq!(val.len(), encoded_size as usize);
      let err = val.write(&[1]).unwrap_err();
      assert_eq!(
        std::string::ToString::to_string(&err),
        "buffer does not have enough space (remaining 0, want 1)"
      );
      Ok(())
    },
  )
  .unwrap();

  let alice2 = Person {
    id: 2,
    name: std::string::String::from("Alice"),
  };

  let old = l
    .insert_with::<()>(
      1,
      u27::new(5),
      |key| {
        key.write(b"alice").unwrap();
        Ok(())
      },
      encoded_size,
      |val| {
        assert_eq!(val.capacity(), encoded_size as usize);
        assert!(val.is_empty());
        val.write(&alice2.id.to_le_bytes()).unwrap();
        assert_eq!(val.len(), 4);
        assert_eq!(val.remaining(), encoded_size as usize - 4);
        assert_eq!(&*val, alice2.id.to_le_bytes());
        val[..4].copy_from_slice(&alice2.id.to_be_bytes());
        assert_eq!(&*val, alice2.id.to_be_bytes());
        val.write(alice2.name.as_bytes()).unwrap();
        assert_eq!(val.len(), encoded_size as usize);
        let err = val.write(&[1]).unwrap_err();
        assert_eq!(
          std::string::ToString::to_string(&err),
          "buffer does not have enough space (remaining 0, want 1)"
        );
        Ok(())
      },
    )
    .unwrap()
    .unwrap();

  assert_eq!(old.key(), b"alice");
  assert!(old.value().starts_with(&alice.id.to_be_bytes()));

  let ent = l.get(1, b"alice").unwrap();
  assert_eq!(ent.key(), b"alice");
  assert!(ent.value().starts_with(&alice2.id.to_be_bytes()));
}

#[test]
fn test_insert_with() {
  run(|| insert_with(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_insert_with_unify() {
  run(|| insert_with(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_insert_with_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_insert_with_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    insert_with(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_with_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_with(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_insert_with_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    insert_with(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn get_or_remove(l: SkipMap) {
  for i in 0..100 {
    let v = new_value(i);
    l.insert(0, &key(i), &v).unwrap();
  }

  for i in 0..100 {
    let k = key(i);
    let old = l.get_or_remove(0, &k).unwrap().unwrap();
    assert_eq!(old.key(), k);
    assert_eq!(old.value(), new_value(i));

    let old = l.get_or_remove(0, &k).unwrap().unwrap();
    assert_eq!(old.key(), k);
    assert_eq!(old.value(), new_value(i));
  }

  for i in 0..100 {
    let k = key(i);
    let ent = l.get(0, &k).unwrap();
    assert_eq!(ent.key(), k);
    assert_eq!(ent.value(), new_value(i));
  }
}

#[test]
fn test_get_or_remove() {
  run(|| get_or_remove(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_get_or_remove_unify() {
  run(|| get_or_remove(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_get_or_remove_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_get_or_remove_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    get_or_remove(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_remove_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_remove(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_get_or_remove_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    get_or_remove(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn remove(l: SkipMap) {
  for i in 0..100 {
    let v = new_value(i);
    l.insert(0, &key(i), &v).unwrap();
  }

  for i in 0..100 {
    let k = key(i);
    // no race, remove should succeed
    let old = l
      .compare_remove(0, &k, Ordering::SeqCst, Ordering::Acquire)
      .unwrap();
    assert!(old.is_none());

    // key already removed
    let old = l
      .compare_remove(0, &k, Ordering::SeqCst, Ordering::Acquire)
      .unwrap();
    assert!(old.is_none());
  }

  for i in 0..100 {
    let k = key(i);
    let ent = l.get(0, &k);
    assert!(ent.is_none());
  }
}

#[test]
fn test_remove() {
  run(|| remove(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_remove_unify() {
  run(|| remove(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_remove_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_remove_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    remove(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_remove_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    remove(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_remove_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    remove(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}

fn remove2(l: SkipMap) {
  for i in 0..100 {
    let v = new_value(i);
    l.insert(0, &key(i), &v).unwrap();
  }

  for i in 0..100 {
    let k = key(i);
    // not found, remove should succeed
    let old = l
      .compare_remove(1, &k, Ordering::SeqCst, Ordering::Acquire)
      .unwrap();
    assert!(old.is_none());

    // no-race, remove should succeed
    let old = l
      .compare_remove(0, &k, Ordering::SeqCst, Ordering::Acquire)
      .unwrap();
    assert!(old.is_none());
  }

  for i in 0..100 {
    let k = key(i);
    let ent = l.get(0, &k);
    assert!(ent.is_none());
  }
}

#[test]
fn test_remove2() {
  run(|| remove2(SkipMap::with_options(TEST_OPTIONS).unwrap()))
}

#[test]
fn test_remove2_unify() {
  run(|| remove2(SkipMap::with_options(UNIFY_TEST_OPTIONS).unwrap()))
}

#[test]
#[cfg(feature = "memmap")]
#[cfg_attr(miri, ignore)]
fn test_remove2_map_mut() {
  run(|| {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("test_skipmap_remove2_map_mut");
    let open_options = OpenOptions::default()
      .create_new(Some(ARENA_SIZE as u32))
      .read(true)
      .write(true);
    let map_options = MmapOptions::default();
    remove2(SkipMap::map_mut(p, open_options, map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_remove2_map_anon() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    remove2(SkipMap::map_anon(map_options).unwrap());
  })
}

#[test]
#[cfg(feature = "memmap")]
fn test_remove2_map_anon_unify() {
  run(|| {
    let map_options = MmapOptions::default().len(ARENA_SIZE as u32);
    remove2(SkipMap::map_anon_with_options(UNIFY_TEST_OPTIONS, map_options).unwrap());
  })
}
