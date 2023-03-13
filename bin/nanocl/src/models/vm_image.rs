use tabled::Tabled;
use chrono::TimeZone;

use nanocld_client::stubs::vm_image::{VmImage, VmImageResizePayload};

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
  /// Clone a VM image
  Clone {
    /// Name of the VM image
    name: String,
    /// Name of the cloned VM image
    clone_name: String,
  },
  /// Resize a VM image
  Resize(VmImageResizeOpts),
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

#[derive(Clone, Debug, Parser)]
pub struct VmImageResizeOpts {
  #[clap(long)]
  pub shrink: bool,
  pub name: String,
  pub size: u64,
}

impl From<VmImageResizeOpts> for VmImageResizePayload {
  fn from(opts: VmImageResizeOpts) -> Self {
    Self {
      size: opts.size,
      shrink: opts.shrink,
    }
  }
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
  pub format: String,
  pub size: String,
  pub created_at: String,
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
    // Convert the created_at and updated_at to the current timezone
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(item.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let size_virtual = convert_size(item.size_virtual);
    let size_actual = convert_size(item.size_actual);
    let size = format!("{} / {}", size_actual, size_virtual);
    Self {
      name: item.name.to_owned(),
      kind: item.kind,
      format: item.format,
      size,
      created_at: format!("{created_at}"),
    }
  }
}
