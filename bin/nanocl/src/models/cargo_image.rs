use chrono::NaiveDateTime;
use tabled::Tabled;
use clap::{Parser, Subcommand};
use bollard_next::models::ImageSummary;

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
  /// Import a cargo image from a tarball
  Import(CargoImageImportOpts),
}

#[derive(Debug, Parser)]
pub struct CargoImageImportOpts {
  /// path to tar archive
  #[clap(short = 'f')]
  pub(crate) file_path: String,
}

/// Manage container images
#[derive(Debug, Parser)]
#[clap(name = "nanocl cargo image")]
pub struct CargoImageOpts {
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
    let binding = value
      .repo_tags
      .get(0)
      .unwrap_or(&String::from("<none>"))
      .to_owned();
    let vals: Vec<_> = binding.split(':').collect();
    let id = value.id.replace("sha256:", "");
    let id = id[0..12].to_owned();
    let created = NaiveDateTime::from_timestamp_opt(value.created, 0).unwrap();
    let created = created.format("%Y-%m-%d %H:%M:%S").to_string();
    let size = value.size;
    let size_string = convert_size(size.try_into().unwrap());

    Self {
      repositories: vals.first().unwrap_or(&"<none>").to_string(),
      tag: vals.get(1).unwrap_or(&"<none>").to_string(),
      image_id: id,
      created,
      size: size_string,
    }
  }
}
