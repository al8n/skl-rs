use skl::{
  badger::{BadgerKey, BadgerValue},
  SkipMap, Value,
};
use std::sync::Arc;

pub fn key(i: usize) -> BadgerKey {
  BadgerKey::from(format!("{:05}", i))
}

pub fn new_value(i: usize) -> BadgerValue {
  use bytes::{BufMut, BytesMut};

  let mut vm = BytesMut::default();
  vm.put_slice(format!("{:05}", i).as_bytes());
  BadgerValue::from(vm.freeze())
}

fn main() {
  const N: usize = 1000;
  let l = Arc::new(SkipMap::new(1 << 20).unwrap());
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
