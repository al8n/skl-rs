use crate::ArenaError;

/// Error type for the [`SkipSet`](crate::SkipSet).
#[derive(Debug)]
pub enum Error {
  /// Indicates the arena is full.
  Full(ArenaError),
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    match self {
      Self::Full(e) => write!(f, "{e}"),
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
