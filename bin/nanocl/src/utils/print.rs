use tabled::Table;
use tabled::settings::object::Segment;
use tabled::settings::{Style, Modify, Padding, Alignment};

use nanocl_utils::io_error::{IoResult, FromIo};

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
