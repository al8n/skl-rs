use crate::ArenaError;

/// Error type for the [`SkipMap`](crate::SkipMap).
///
/// [`SkipMap`]: crate::SkipMap
#[derive(Debug)]
pub enum Error {
  /// Indicates that the arena is full
  Full(ArenaError),

  /// Indicates that an entry with the specified key already
  /// exists in the skiplist. As a low-level crate, duplicate entries are not directly supported and
  /// instead must be handled by the user.
  Duplicated,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::Full(e) => write!(f, "{e}"),
      Self::Duplicated => write!(f, "key already exists in the skiplist"),
    }
  }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl From<ArenaError> for Error {
  fn from(e: ArenaError) -> Self {
    Self::Full(e)
  }
}
