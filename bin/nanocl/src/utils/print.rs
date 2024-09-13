use futures::StreamExt;
use ntex::channel::mpsc::Receiver;
use tabled::settings::object::Segment;
use tabled::settings::{Alignment, Modify, Padding, Style};
use tabled::Table;

use nanocl_error::http::HttpError;
use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::process::{OutputKind, ProcessOutputLog};

use crate::models::DisplayFormat;

/// Print a table from an iterator of [Tabled](tabled::Tabled) elements
pub(crate) fn print_table<T>(iter: impl IntoIterator<Item = T>)
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
pub(crate) fn print_yml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let yml = serde_yaml::to_string(&data)
    .map_err(|err| err.map_err_context(|| "Print yaml"))?;
  print!("{yml}");
  Ok(())
}

/// Print json from a serializable data
pub(crate) fn print_json<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let json = serde_json::to_string_pretty(&data)
    .map_err(|err| err.map_err_context(|| "Print json"))?;
  print!("{json}");
  Ok(())
}

/// Print toml from a serializable data
pub(crate) fn print_toml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let toml = toml::to_string(&data).map_err(|err| {
    IoError::with_context(
      "Print toml",
      std::io::Error::new(std::io::ErrorKind::InvalidData, err),
    )
  })?;
  print!("{toml}");
  Ok(())
}

/// Display data in a specific format
pub(crate) fn display_format<T>(format: &DisplayFormat, data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  match format {
    DisplayFormat::Yaml => print_yml(data),
    DisplayFormat::Toml => print_toml(data),
    DisplayFormat::Json => print_json(data),
  }
}

pub(crate) async fn logs_process_stream(
  stream: Receiver<Result<ProcessOutputLog, HttpError>>,
) -> IoResult<()> {
  let mut stream = stream;
  while let Some(s) = stream.next().await {
    let s = match s {
      Ok(s) => s,
      Err(e) => return Err(e.map_err_context(|| "Stream").into()),
    };
    let output = format!("[{}] {}", &s.name, &s.log.data);
    match s.log.kind {
      OutputKind::StdOut => {
        print!("{output}");
      }
      OutputKind::StdErr => {
        eprint!("{output}");
      }
      OutputKind::Console => print!("{output}"),
      _ => {}
    }
  }
  Ok(())
}
