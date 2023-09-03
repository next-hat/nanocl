use tabled::Table;
use tabled::settings::object::Segment;
use tabled::settings::{Style, Modify, Padding, Alignment};

use nanocl_utils::io_error::{IoResult, FromIo, IoError};

use crate::models::DisplayFormat;

/// ## Print table
///
/// Print a table from an iterator of tabled::Tabled
///
/// ## Arguments
///
/// - [iter](impl IntoIterator<Item = T>) The iterator of tabled::Tabled
///
pub fn print_table<T>(iter: impl IntoIterator<Item = T>)
where
  T: tabled::Tabled,
{
  let table = Table::new(iter)
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 4, 0, 0))
        .with(Alignment::left()),
    )
    .to_string();
  println!("{table}");
}

pub fn print_yml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let yml = serde_yaml::to_string(&data)
    .map_err(|err| err.map_err_context(|| "Print yaml"))?;
  print!("{yml}");
  Ok(())
}

/// ## Print json
///
/// Print json from a serializable data
///
/// ## Arguments
///
/// - [data](T) The serializable data
///
/// ## Returns
///
/// - [Result](Result) The result of the operation
///   - [Ok](()) The operation was successful
///   - [Err](IoError) An error occured
///
pub fn print_json<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let json = serde_json::to_string_pretty(&data)
    .map_err(|err| err.map_err_context(|| "Print json"))?;
  print!("{json}");
  Ok(())
}

/// ## Print toml
///
/// Print toml from a serializable data
///
/// ## Arguments
///
/// - [data](T) The serializable data
///
/// ## Returns
///
/// - [Result](Result) The result of the operation
///   - [Ok](()) The operation was successful
///   - [Err](IoError) An error occured
///
pub fn print_toml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let toml = toml::to_string(&data).map_err(|err| {
    IoError::new(
      "Print toml",
      std::io::Error::new(std::io::ErrorKind::InvalidData, err),
    )
  })?;
  print!("{toml}");
  Ok(())
}

/// ## Display format
///
/// Display data in a specific format
///
/// ## Arguments
///
/// - [format](DisplayFormat) The format to display the data
/// - [data](T) The serializable data
///
/// ## Returns
///
/// - [Result](Result) The result of the operation
///   - [Ok](()) The operation was successful
///   - [Err](IoError) An error occured
///
pub fn display_format<T>(format: &DisplayFormat, data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  match format {
    DisplayFormat::Yaml => print_yml(data),
    DisplayFormat::Toml => print_toml(data),
    DisplayFormat::Json => print_json(data),
  }
}
