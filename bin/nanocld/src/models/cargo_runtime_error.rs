use std::{fmt, io};

use nanocl_error::io::IoError;

use super::RuntimeError;

/// Cargo-specific evidence for rollout recovery and operator diagnostics.
#[derive(Debug)]
pub(crate) enum CargoRuntimeError {
  /// A tracked candidate's failure can be settled by a verified rollback.
  Candidate(io::Error),
  /// A remote node reported a reference verified against the accepted spec.
  RemoteMissingSecret(String),
  /// Docker confirmed a nonzero init-container exit.
  InitExit(i64),
}

impl CargoRuntimeError {
  pub(crate) fn get(error: &IoError) -> Option<&Self> {
    RuntimeError::cause(error).get_ref()?.downcast_ref()
  }

  pub(crate) fn candidate(error: impl Into<IoError>) -> IoError {
    let mut error = error.into();
    error.inner =
      io::Error::new(error.inner.kind(), Self::Candidate(error.inner));
    RuntimeError::unconfirmed(error)
  }

  pub(crate) fn remote_missing_secret(name: String) -> IoError {
    RuntimeError::unconfirmed(IoError::with_context(
      "Cargo remote missing secret",
      io::Error::new(
        io::ErrorKind::Interrupted,
        Self::RemoteMissingSecret(name),
      ),
    ))
  }

  pub(crate) fn init_exit(code: i64) -> IoError {
    IoError::with_context(
      "CargoInitContainerExit",
      io::Error::new(io::ErrorKind::Interrupted, Self::InitExit(code)),
    )
  }

  /// Preflight and verified rollback can turn a candidate observation into a
  /// definitive failure. Keep the underlying cause for the new context.
  pub(crate) fn settle_candidate(context: &str, error: IoError) -> IoError {
    let uncertain = RuntimeError::is_uncertain(&error);
    let mut error = RuntimeError::into_cause(error);
    error.context = Some(context.to_owned());
    let kind = error.inner.kind();
    error.inner = match error.inner.downcast::<Self>() {
      Ok(Self::Candidate(cause)) => {
        return IoError::with_context(context, cause);
      }
      Ok(other) => io::Error::new(kind, other),
      Err(original) => original,
    };
    if uncertain {
      RuntimeError::unconfirmed(error)
    } else {
      error
    }
  }
}

impl fmt::Display for CargoRuntimeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Candidate(cause) => cause.fmt(f),
      Self::RemoteMissingSecret(name) => {
        write!(f, "A node reported that secret {name:?} does not exist")
      }
      Self::InitExit(code) => {
        write!(f, "Init container exited with code {code}")
      }
    }
  }
}

impl std::error::Error for CargoRuntimeError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Candidate(cause) => Some(cause),
      Self::InitExit(_) | Self::RemoteMissingSecret(_) => None,
    }
  }
}
