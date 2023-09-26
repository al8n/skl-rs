pub mod badger {
  use skl::{badger::*, bytes};

  /// Only used for testing
  pub fn key(i: usize) -> BadgerKey {
    BadgerKey::from(format!("{:05}", i))
  }

  /// Only used for testing
  pub fn big_value(i: usize) -> BadgerValue {
    BadgerValue::from(format!("{:01048576}", i))
  }

  /// Only used for testing
  pub fn new_value(i: usize) -> BadgerValue {
    use bytes::{BufMut, BytesMut};

    let mut vm = BytesMut::default();
    vm.put_slice(format!("{:05}", i).as_bytes());
    BadgerValue::from(vm.freeze())
  }
}
