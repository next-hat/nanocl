use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::vm_image::{VmImage, VmImageResizePayload};

/// ## VmImageCreateOpts
///
/// Create a base VM image options
///
#[derive(Debug, Parser)]
pub struct VmImageCreateOpts {
  /// Name of the VM image
  pub name: String,
  /// Path or url to the VM image
  pub file_path: String,
}

/// ## VmImageCommands
///
/// Available virtual machine image commands
///
#[derive(Debug, Subcommand)]
pub enum VmImageCommand {
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
  List(VmImageListOpts),
  /// Remove a VM image
  #[clap(alias("rm"))]
  Remove {
    /// Names of the VM image
    names: Vec<String>,
  },
}

/// ## VmImageListOpts
///
/// List VM images options
///
#[derive(Clone, Debug, Parser)]
pub struct VmImageListOpts {
  /// Show only images name
  #[clap(long, short)]
  pub quiet: bool,
}

/// ## VmImageResizeOpts
///
/// Resize a VM image options
///
#[derive(Clone, Debug, Parser)]
pub struct VmImageResizeOpts {
  /// Shrink the image
  #[clap(long)]
  pub shrink: bool,
  /// Name of the VM image
  pub name: String,
  /// New size of the VM image
  pub size: u64,
}

/// Convert VmImageResizeOpts to VmImageResizePayload
impl From<VmImageResizeOpts> for VmImageResizePayload {
  fn from(opts: VmImageResizeOpts) -> Self {
    Self {
      size: opts.size,
      shrink: opts.shrink,
    }
  }
}

/// ## VmImageArgs
///
/// Vm image arguments
///
#[derive(Debug, Parser)]
pub struct VmImageArgs {
  /// Command to run
  #[clap(subcommand)]
  pub command: VmImageCommand,
}

/// ## VmImageRow
///
/// A row for the vm image table
///
#[derive(Tabled)]
pub struct VmImageRow {
  /// Name of the VM image
  pub name: String,
  /// Kind of the VM image
  pub kind: String,
  /// Format of the VM image
  pub format: String,
  /// Size of the VM image
  pub size: String,
  /// When the VM image was created
  pub created_at: String,
}

/// Convert size to human readable format
fn convert_size(size: i64) -> String {
  if size >= 1_000_000_000 {
    format!("{} GB", size / 1024 / 1024 / 1024)
  } else {
    format!("{} MB", size / 1024 / 1024)
  }
}

/// Convert VmImage to VmImageRow
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
