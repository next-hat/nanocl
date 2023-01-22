use chrono::NaiveDateTime;
use tabled::Tabled;
use clap::{Parser, Subcommand};
use bollard::models::ImageSummary;

#[derive(Debug, Parser)]
pub struct CargoImageRemoveOpts {
  /// id or name of image to delete
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct CargoImageCreateOpts {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct CargoImageInspectOpts {
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum CargoImageCommands {
  /// List cargo images
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo image
  Create(CargoImageCreateOpts),
  /// Remove an existing cargo image
  #[clap(alias("rm"))]
  Remove(CargoImageRemoveOpts),
  /// Inspect a cargo image
  Inspect(CargoImageInspectOpts),
}

/// Manage container images
#[derive(Debug, Parser)]
pub struct CargoImageArgs {
  #[clap(subcommand)]
  pub(crate) commands: CargoImageCommands,
}

#[derive(Tabled)]
pub struct CargoImageRow {
  pub(crate) repositories: String,
  pub(crate) tag: String,
  pub(crate) image_id: String,
  pub(crate) created: String,
  pub(crate) size: String,
}

fn convert_size(size: u64) -> String {
  if size >= 1_000_000_000 {
    format!("{} GB", size / 1_000_000_000)
  } else {
    format!("{} MB", size / 1_000_000)
  }
}

impl From<ImageSummary> for CargoImageRow {
  fn from(value: ImageSummary) -> Self {
    let binding = value.repo_tags[0].to_owned();
    let vals: Vec<_> = binding.split(':').collect();
    let id = value.id.replace("sha256:", "");
    let id = id[0..12].to_owned();
    let created = NaiveDateTime::from_timestamp_opt(value.created, 0).unwrap();
    let created = created.format("%Y-%m-%d %H:%M:%S").to_string();
    let size = value.size;
    let size_string = convert_size(size.try_into().unwrap());

    Self {
      repositories: vals[0].to_owned(),
      tag: vals[1].to_owned(),
      image_id: id,
      created,
      size: size_string,
    }
  }
}
