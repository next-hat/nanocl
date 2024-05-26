use std::collections::HashMap;

use nanocl_error::io::{IoError, FromIo};

mod read;
mod create;
mod update;
mod delete;
mod with_spec;

pub use read::*;
pub use create::*;
pub use update::*;
pub use delete::*;
pub use with_spec::*;

use crate::models::ColumnType;

pub trait RepositoryBase {
  /// Get the name of the current type
  fn get_name() -> &'static str {
    let name = std::any::type_name::<Self>();
    name.split("::").last().unwrap_or(name)
  }

  /// Map an error with the context of the current type name
  fn map_err<E>(err: E) -> Box<IoError>
  where
    E: FromIo<Box<IoError>>,
  {
    err.map_err_context(Self::get_name)
  }

  /// Get the columns of the current type as a hashmap
  /// This help to generate the sql queries based on the generic filter
  fn get_columns<'a>() -> HashMap<&'a str, (ColumnType, &'a str)> {
    HashMap::new()
  }
}
