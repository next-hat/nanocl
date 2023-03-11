use clap::{Parser, Subcommand};
use nanocld_client::stubs::vm_config::VmConfigPartial;

use super::VmImageArgs;

#[derive(Debug, Subcommand)]
pub enum VmCommands {
  /// Manage vm images
  Image(VmImageArgs),
  Create(VmCreateOpts),
}

#[derive(Clone, Debug, Parser)]
pub struct VmCreateOpts {
  /// Name of the vm
  pub name: String,
  /// Name of the vm image
  pub image: String,
}

impl From<VmCreateOpts> for VmConfigPartial {
  fn from(val: VmCreateOpts) -> Self {
    Self {
      name: val.name,
      image: val.image,
      hostname: None,
      cpu: None,
      memory: None,
      net_iface: None,
    }
  }
}

/// Manage configuration states
#[derive(Debug, Parser)]
pub struct VmArgs {
  /// namespace to target by default global is used
  #[clap(long, short)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: VmCommands,
}
