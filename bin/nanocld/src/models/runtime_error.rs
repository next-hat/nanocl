use std::{fmt, io};

use nanocl_error::io::IoError;

/// The runtime outcome is unconfirmed. Keep the original cause and I/O kind
/// so each workload can apply its own observation and recovery rules.
#[derive(Debug)]
pub(crate) struct RuntimeError(io::Error);

impl RuntimeError {
  pub(crate) fn unconfirmed(error: impl Into<IoError>) -> IoError {
    let mut error = error.into();
    if !Self::is_uncertain(&error) {
      error.inner = io::Error::new(error.inner.kind(), Self(error.inner));
    }
    error
  }

  /// Inspect the original IoError before cloning it for display.
  pub(crate) fn is_uncertain(error: &IoError) -> bool {
    error
      .inner
      .get_ref()
      .is_some_and(|cause| cause.is::<Self>())
  }

  /// Look through the uncertainty marker for a workload-specific diagnostic.
  pub(crate) fn cause(error: &IoError) -> &io::Error {
    match error
      .inner
      .get_ref()
      .and_then(|cause| cause.downcast_ref::<Self>())
    {
      Some(Self(cause)) => cause,
      None => &error.inner,
    }
  }

  /// After observation establishes the outcome, restore the original error.
  pub(crate) fn into_cause(mut error: IoError) -> IoError {
    error.inner = match error.inner.downcast::<Self>() {
      Ok(Self(cause)) | Err(cause) => cause,
    };
    error
  }
}

impl fmt::Display for RuntimeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    // IoError normally recognizes native OS errors at the outermost layer
    // and normalizes their wording. Retain that formatting when tagged.
    if let Some(code) = self.0.raw_os_error() {
      IoError::without_context(io::Error::from_raw_os_error(code)).fmt(f)
    } else {
      self.0.fmt(f)
    }
  }
}

impl std::error::Error for RuntimeError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    Some(&self.0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn confirmation_restores_the_original_error_after_repeated_uncertainty() {
    let original = IoError::with_context(
      "Inspect runtime",
      io::Error::from_raw_os_error(libc::ENOENT),
    );
    let display = original.to_string();
    let error = RuntimeError::unconfirmed(RuntimeError::unconfirmed(original));
    assert!(RuntimeError::is_uncertain(&error));
    assert_eq!(error.inner.kind(), io::ErrorKind::NotFound);
    assert_eq!(error.to_string(), display);
    assert_eq!(
      RuntimeError::cause(&error).raw_os_error(),
      Some(libc::ENOENT)
    );

    let confirmed = RuntimeError::into_cause(error);
    assert!(!RuntimeError::is_uncertain(&confirmed));
    assert_eq!(confirmed.inner.raw_os_error(), Some(libc::ENOENT));
    assert_eq!(confirmed.to_string(), display);
  }
}
