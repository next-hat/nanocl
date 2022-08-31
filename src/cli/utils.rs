use tabled::{
  object::{Segment, Rows},
  Padding, Alignment, Table, Style, Modify,
};

use crate::models::{YmlFile, YmlConfigTypes};

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
  print!("{}", table);
}

pub fn get_config_type(str: &str) -> Result<YmlConfigTypes, serde_yaml::Error> {
  let result = serde_yaml::from_str::<YmlFile>(str)?;
  Ok(result.file_type)
}
