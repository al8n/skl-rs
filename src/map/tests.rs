use super::*;
use crate::{badger::BadgerKey, sync::Arc, value::badger::BadgerValue};
use alloc::format;
use bytes::{BufMut, BytesMut};

const ARENA_SIZE: usize = 1 << 20;

/// Only used for testing

pub fn key(i: usize) -> BadgerKey {
  BadgerKey::from(format!("{:05}", i)).with_version(0)
}

/// Only used for testing
pub fn big_value(i: usize) -> BadgerValue {
  BadgerValue::from(format!("{:01048576}", i))
}

/// Only used for testing
pub fn new_value(i: usize) -> BadgerValue {
  let mut vm = BytesMut::new();
  vm.put_slice(format!("{:05}", i).as_bytes());
  vm.freeze().into()
}

fn test_basic_runner(l: Arc<SkipMap<BadgerKey, BadgerValue>>) {
  let mut v1 = new_value(42);
  let mut v2 = new_value(52);
  let mut v3 = new_value(62);
  let mut v4 = new_value(72);
  let mut v5 = {
    let mut vm = BytesMut::default();
    vm.put_slice(format!("{:0102400}", 1).as_bytes());
    BadgerValue::from(vm.freeze())
  };

  // Have size 100 KB which is > math.MaxUint16.
  // Try inserting values.
  v1.set_meta(55);
  l.insert(&BadgerKey::from("key1".as_bytes().to_vec()), &v1)
    .unwrap();
  v2.set_meta(56);
  l.insert(&BadgerKey::from("key2".as_bytes()).with_version(2), &v2)
    .unwrap();
  v3.set_meta(57);
  l.insert(&BadgerKey::from("key3".as_bytes()), &v3).unwrap();

  assert!(l.get(&BadgerKey::from("key".as_bytes())).is_none());

  let key = BadgerKey::from("key1".as_bytes());
  let v = l.get(&key).unwrap();
  assert_eq!(v.as_bytes(), "00042".as_bytes());
  assert_eq!(v.trailer().meta(), 55);
  assert!(l.get(&BadgerKey::from("key2".as_bytes())).is_none());

  let key = BadgerKey::from("key3".as_bytes());
  let v = l.get(&key).unwrap();
  assert_eq!(v.as_bytes(), "00062".as_bytes());
  assert_eq!(v.trailer().meta(), 57);

  v4.set_meta(12);
  l.insert(&BadgerKey::from("key3".as_bytes()).with_version(1), &v4)
    .unwrap();
  let key = BadgerKey::from("key3".as_bytes()).with_version(1);
  let v = l.get(&key).unwrap();
  assert_eq!(v.as_bytes(), "00072".as_bytes());
  assert_eq!(v.trailer().meta(), 12);

  v5.set_meta(60);
  l.insert(
    &BadgerKey::from("key4".as_bytes()).with_version(1),
    &v5.clone(),
  )
  .unwrap();

  let key = BadgerKey::from("key4".as_bytes()).with_version(1);
  let v = l.get(&key).unwrap();
  assert_eq!(v.as_bytes(), v5.as_bytes());
  assert_eq!(v.trailer().meta(), 60);
}

#[test]
fn test_basic() {
  let l = Arc::new(SkipMap::<BadgerKey, BadgerValue>::new(ARENA_SIZE).unwrap());
  test_basic_runner(l);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_basic_mmap() {
  let l = Arc::new(SkipMap::mmap(ARENA_SIZE, tempfile::tempfile().unwrap(), true).unwrap());
  test_basic_runner(l);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_basic_mmap_anon() {
  let l = Arc::new(SkipMap::mmap_anon(ARENA_SIZE).unwrap());
  test_basic_runner(l);
}

fn test_basic_large_testcases_in(l: Arc<SkipMap<BadgerKey, BadgerValue>>) {
  let n = 1000;

  for i in 0..n {
    l.insert(&key(i), &new_value(i)).unwrap();
  }

  for i in 0..n {
    let k = key(i);
    let v = l.get(&k).unwrap();
    assert_eq!(new_value(i).as_bytes(), v.as_bytes());
  }

  assert_eq!(n, l.len());
}

#[test]
fn test_basic_large_testcases() {
  let l = Arc::new(SkipMap::<BadgerKey, BadgerValue>::new(ARENA_SIZE).unwrap());
  test_basic_large_testcases_in(l);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_basic_large_testcases_mmap() {
  let l = Arc::new(SkipMap::mmap(ARENA_SIZE, tempfile::tempfile().unwrap(), true).unwrap());
  test_basic_large_testcases_in(l);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_basic_large_testcases_mmap_anon() {
  let l = Arc::new(SkipMap::mmap_anon(ARENA_SIZE).unwrap());
  test_basic_large_testcases_in(l);
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_mmap_anon() {
  test_concurrent_basic_runner(Arc::new(SkipMap::mmap_anon(ARENA_SIZE).unwrap()));
}

fn test_concurrent_basic_runner(l: Arc<SkipMap<BadgerKey, BadgerValue>>) {
  #[cfg(miri)]
  const N: usize = 100;
  #[cfg(not(miri))]
  const N: usize = 1000;

  let wg = Arc::new(());
  for i in 0..N {
    let w = wg.clone();
    let l = l.clone();
    std::thread::spawn(move || {
      l.insert(&key(i), &new_value(i)).unwrap();
      drop(w);
    });
  }
  while Arc::strong_count(&wg) > 1 {}
  for i in 0..N {
    let w = wg.clone();
    let l = l.clone();
    std::thread::spawn(move || {
      let k = key(i);
      assert_eq!(
        l.get(&k).unwrap().as_bytes(),
        new_value(i).as_bytes(),
        "broken: {i}"
      );
      drop(w);
    });
  }
}

fn test_concurrent_basic_big_values_runner(l: Arc<SkipMap<BadgerKey, BadgerValue>>) {
  #[cfg(miri)]
  const N: usize = 10;
  #[cfg(not(miri))]
  const N: usize = 100;

  for i in 0..N {
    let l = l.clone();
    std::thread::spawn(move || {
      l.insert(&key(i), &big_value(i)).unwrap();
    });
  }
  while Arc::strong_count(&l) > 1 {}
  assert_eq!(N, l.len());
  for i in 0..N {
    let l = l.clone();
    std::thread::spawn(move || {
      let k = key(i);
      assert_eq!(
        l.get(&k).unwrap().as_bytes(),
        big_value(i).as_bytes(),
        "broken: {i}"
      );
    });
  }
  while Arc::strong_count(&l) > 1 {}
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_big_values() {
  test_concurrent_basic_big_values_runner(Arc::new(SkipMap::new(120 << 20).unwrap()));
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_big_values_mmap() {
  test_concurrent_basic_big_values_runner(Arc::new(
    SkipMap::mmap(120 << 20, tempfile::tempfile().unwrap(), true).unwrap(),
  ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn test_concurrent_basic_big_values_mmap_anon() {
  test_concurrent_basic_big_values_runner(Arc::new(SkipMap::mmap_anon(120 << 20).unwrap()));
}
