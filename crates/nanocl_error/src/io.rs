#[derive(Debug)]
pub struct IoError {
  pub context: Option<String>,
  pub inner: std::io::Error,
}

impl Clone for IoError {
  fn clone(&self) -> Self {
    Self {
      context: self.context.clone(),
      inner: std::io::Error::new(self.inner.kind(), self.inner.to_string()),
    }
  }
}

impl IoError {
  pub fn new<T>(context: T, inner: std::io::Error) -> Self
  where
    T: Into<String>,
  {
    Self {
      context: Some(context.into()),
      inner,
    }
  }

  pub fn without_context(inner: std::io::Error) -> Self {
    Self {
      context: None,
      inner,
    }
  }

  pub fn invalid_data<M>(context: M, message: M) -> Self
  where
    M: ToString + std::fmt::Display,
  {
    Self::new(
      context.to_string(),
      std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string()),
    )
  }

  pub fn invalid_input<M>(context: M, message: M) -> Self
  where
    M: ToString + std::fmt::Display,
  {
    Self::new(
      context.to_string(),
      std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.to_string(),
      ),
    )
  }

  pub fn not_found<M>(context: M, message: M) -> Self
  where
    M: ToString + std::fmt::Display,
  {
    Self::new(
      context.to_string(),
      std::io::Error::new(std::io::ErrorKind::NotFound, message.to_string()),
    )
  }

  pub fn interupted<M>(context: M, message: M) -> Self
  where
    M: ToString + std::fmt::Display,
  {
    Self::new(
      context.to_string(),
      std::io::Error::new(std::io::ErrorKind::Interrupted, message.to_string()),
    )
  }

  pub fn context(&self) -> Option<&str> {
    self.context.as_deref()
  }

  pub fn into_inner(self) -> std::io::Error {
    self.inner
  }

  pub fn exit(&self) -> ! {
    std::process::exit(self.inner.raw_os_error().unwrap_or(1));
  }
}

impl std::fmt::Display for IoError {
  fn fmt(
    &self,
    f: &mut std::fmt::Formatter<'_>,
  ) -> Result<(), std::fmt::Error> {
    use std::io::ErrorKind::*;

    let mut message;
    let message = if self.inner.raw_os_error().is_some() {
      // These are errors that come directly from the OS.
      // We want to normalize their messages across systems,
      // and we want to strip the "(os error X)" suffix.
      match self.inner.kind() {
        NotFound => "No such file or directory",
        PermissionDenied => "Permission denied",
        ConnectionRefused => "Connection refused",
        ConnectionReset => "Connection reset",
        ConnectionAborted => "Connection aborted",
        NotConnected => "Not connected",
        AddrInUse => "Address in use",
        AddrNotAvailable => "Address not available",
        BrokenPipe => "Broken pipe",
        AlreadyExists => "Already exists",
        WouldBlock => "Would block",
        InvalidInput => "Invalid input",
        InvalidData => "Invalid data",
        TimedOut => "Timed out",
        WriteZero => "Write zero",
        Interrupted => "Interrupted",
        UnexpectedEof => "Unexpected end of file",
        _ => {
          // TODO: When the new error variants
          // (https://github.com/rust-lang/rust/issues/86442)
          // are stabilized, we should add them to the match statement.
          message = strip_errno(&self.inner);
          capitalize(&mut message);
          &message
        }
      }
    } else {
      // These messages don't need as much normalization, and the above
      // messages wouldn't always be a good substitute.
      // For example, ErrorKind::NotFound doesn't necessarily mean it was
      // a file that was not found.
      // There are also errors with entirely custom messages.
      message = self.inner.to_string();
      capitalize(&mut message);
      &message
    };
    if let Some(ctx) = &self.context {
      write!(f, "{ctx}: {message}")
    } else {
      write!(f, "{message}")
    }
  }
}

impl std::error::Error for IoError {}

/// Capitalize the first character of an ASCII string.
fn capitalize(text: &mut str) {
  if let Some(first) = text.get_mut(..1) {
    first.make_ascii_uppercase();
  }
}

/// Strip the trailing " (os error XX)" from io error strings.
fn strip_errno(err: &std::io::Error) -> String {
  let mut msg = err.to_string();
  if let Some(pos) = msg.find(" (os error ") {
    msg.truncate(pos);
  }
  msg
}

pub type IoResult<T, E = IoError> = Result<T, E>;

/// Enables the conversion from [`std::io::Error`] to [`IoError`] and from [`std::io::Result`] to [`IoResult`].
pub trait FromIo<T> {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> T
  where
    C: ToString + std::fmt::Display;
}

impl FromIo<IoError> for IoError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> IoError
  where
    C: ToString + std::fmt::Display,
  {
    IoError {
      context: Some((context)().to_string()),
      inner: self.into_inner(),
    }
  }
}

impl FromIo<Box<IoError>> for std::io::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: self,
    })
  }
}

impl FromIo<Box<IoError>> for std::string::FromUtf8Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(std::io::ErrorKind::InvalidData, self),
    })
  }
}

impl From<Box<IoError>> for IoError {
  fn from(f: Box<IoError>) -> Self {
    *f
  }
}

impl From<std::io::Error> for IoError {
  fn from(f: std::io::Error) -> Self {
    Self {
      context: None,
      inner: f,
    }
  }
}

impl From<IoError> for std::io::Error {
  fn from(f: IoError) -> Self {
    f.inner
  }
}

#[cfg(feature = "serde_json")]
impl FromIo<Box<IoError>> for serde_json::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(std::io::ErrorKind::InvalidData, self),
    })
  }
}

#[cfg(feature = "serde_yaml")]
impl FromIo<Box<IoError>> for serde_yaml::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(std::io::ErrorKind::InvalidData, self),
    })
  }
}

#[cfg(feature = "serde_urlencoded")]
impl FromIo<Box<IoError>> for serde_urlencoded::ser::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{self}"),
      ),
    })
  }
}

#[cfg(feature = "bollard")]
impl FromIo<Box<IoError>> for bollard_next::errors::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(std::io::ErrorKind::InvalidData, self),
    })
  }
}

#[cfg(feature = "http")]
impl From<crate::http::HttpError> for IoError {
  fn from(f: crate::http::HttpError) -> Self {
    Self {
      context: None,
      inner: std::io::Error::new(std::io::ErrorKind::InvalidData, f),
    }
  }
}

#[cfg(feature = "diesel")]
impl FromIo<Box<IoError>> for diesel::result::Error {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    let inner = match self {
      diesel::result::Error::NotFound => {
        std::io::Error::new(std::io::ErrorKind::NotFound, self)
      }
      diesel::result::Error::DatabaseError(dberr, infoerr) => match dberr {
        diesel::result::DatabaseErrorKind::UniqueViolation => {
          std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            infoerr.details().unwrap_or_default(),
          )
        }
        _ => std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          infoerr.details().unwrap_or_default(),
        ),
      },
      _ => std::io::Error::new(std::io::ErrorKind::InvalidData, self),
    };
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner,
    })
  }
}

#[cfg(feature = "ntex")]
impl From<ntex::http::error::BlockingError<IoError>> for IoError {
  fn from(f: ntex::http::error::BlockingError<IoError>) -> Self {
    match f {
      ntex::http::error::BlockingError::Error(e) => e,
      ntex::http::error::BlockingError::Canceled => {
        IoError::interupted("Future", "Canceled")
      }
    }
  }
}

#[cfg(feature = "ntex")]
impl FromIo<Box<IoError>> for ntex::http::client::error::SendRequestError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    let inner = match self {
      ntex::http::client::error::SendRequestError::Timeout => {
        std::io::Error::new(std::io::ErrorKind::TimedOut, format!("{self}"))
      }
      ntex::http::client::error::SendRequestError::Connect(err) => match err {
        ntex::http::client::error::ConnectError::Disconnected(_) => {
          std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            format!("{err}"),
          )
        }
        _ => std::io::Error::new(
          std::io::ErrorKind::ConnectionRefused,
          format!("{err}"),
        ),
      },
      _ => {
        std::io::Error::new(std::io::ErrorKind::Interrupted, format!("{self}"))
      }
    };
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner,
    })
  }
}

#[cfg(feature = "ntex")]
impl FromIo<Box<IoError>> for ntex::http::client::error::JsonPayloadError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{self}"),
      ),
    })
  }
}

#[cfg(feature = "ntex")]
impl FromIo<Box<IoError>> for ntex::http::error::PayloadError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{self}"),
      ),
    })
  }
}

#[cfg(feature = "ntex")]
impl FromIo<Box<IoError>> for ntex::ws::error::WsClientBuilderError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{self}"),
      ),
    })
  }
}

#[cfg(feature = "ntex")]
impl FromIo<Box<IoError>> for ntex::ws::error::WsClientError {
  fn map_err_context<C>(self, context: impl FnOnce() -> C) -> Box<IoError>
  where
    C: ToString + std::fmt::Display,
  {
    Box::new(IoError {
      context: Some((context)().to_string()),
      inner: std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{self}"),
      ),
    })
  }
}
