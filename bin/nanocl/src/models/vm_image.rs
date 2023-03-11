use tabled::Tabled;

use nanocld_client::stubs::vm_image::VmImage;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub struct VmImageCreateOpts {
  pub name: String,
  /// Path or url to the state
  pub file_path: String,
}

#[derive(Debug, Subcommand)]
pub enum VmImageCommands {
  /// Create a base VM image
  Create(VmImageCreateOpts),
  /// List VM images
  #[clap(alias("ls"))]
  List,
  /// Remove a VM image
  #[clap(alias("rm"))]
  Remove {
    /// Names of the VM image
    names: Vec<String>,
  },
}

/// Manage configuration states
#[derive(Debug, Parser)]
pub struct VmImageArgs {
  #[clap(subcommand)]
  pub commands: VmImageCommands,
}

#[derive(Tabled)]
pub struct VmImageRow {
  pub name: String,
  pub kind: String,
  pub size: String,
  pub created_at: String,
  pub path: String,
}

fn convert_size(size: i64) -> String {
  if size >= 1_000_000_000 {
    format!("{} GB", size / 1024 / 1024 / 1024)
  } else {
    format!("{} MB", size / 1024 / 1024)
  }
}

impl From<VmImage> for VmImageRow {
  fn from(item: VmImage) -> Self {
    let created_at = item.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
    Self {
      name: item.name.to_owned(),
      kind: item.kind.to_owned(),
      size: convert_size(item.size),
      created_at,
      path: item.path,
    }
  }
}
