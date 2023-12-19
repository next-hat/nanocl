use nanocl_error::io::{IoError, FromIo};

mod create;
mod read;
mod update;
mod delete;

pub use create::RepositoryCreate;
pub use read::RepositoryRead;
pub use delete::RepositoryDelete;

pub trait RepositoryBase {
  // fn map_err
  /// Get the name of the current type
  fn get_name() -> &'static str {
    let name = std::any::type_name::<Self>();
    let short = name.split("::").last().unwrap_or(name);
    short
  }

  /// Map an error with the context of the current type name
  fn map_err<E>(err: E) -> Box<IoError>
  where
    E: FromIo<Box<IoError>>,
  {
    err.map_err_context(Self::get_name)
  }
}
