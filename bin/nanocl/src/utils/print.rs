use tabled::{
  object::{Segment, Rows},
  Padding, Alignment, Table, Style, Modify,
};

use crate::error::CliError;

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
    .with(Modify::new(Rows::first()).with(str::to_uppercase))
    .to_string();
  println!("{table}");
}

pub fn print_yml<T>(data: T) -> Result<(), CliError>
where
  T: serde::Serialize,
{
  let yml = serde_yaml::to_string(&data)?;
  println!("{yml}");
  Ok(())
}
