use tabled::Table;
use tabled::settings::object::Segment;
use tabled::settings::{Style, Modify, Padding, Alignment};

use nanocl_error::io::{IoResult, FromIo, IoError};

use crate::models::DisplayFormat;

/// Print a table from an iterator of [Tabled](tabled::Tabled) elements
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

/// Print yaml from a serializable data
pub fn print_yml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let yml = serde_yaml::to_string(&data)
    .map_err(|err| err.map_err_context(|| "Print yaml"))?;
  print!("{yml}");
  Ok(())
}

/// Print json from a serializable data
pub fn print_json<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let json = serde_json::to_string_pretty(&data)
    .map_err(|err| err.map_err_context(|| "Print json"))?;
  print!("{json}");
  Ok(())
}

/// Print toml from a serializable data
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

/// Display data in a specific format
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
