use tabled::Tabled;
use chrono::TimeZone;
use clap::{Parser, Subcommand};

use nanocld_client::stubs::{
  vm_config::{VmConfigPartial, VmDiskConfig, VmHostConfig, VmConfigUpdate},
  vm::VmSummary,
};

use super::VmImageArgs;

#[derive(Debug, Subcommand)]
pub enum VmCommands {
  /// Run a vm
  Run(VmRunOpts),
  /// Manage vm images
  Image(VmImageArgs),
  /// Create a vm
  Create(VmCreateOpts),
  /// List vms
  #[clap(alias = "ls")]
  List,
  #[clap(alias = "rm")]
  Remove {
    /// Names of the vm
    names: Vec<String>,
  },
  /// Inspect a vm
  Inspect {
    /// Name of the vm
    name: String,
  },
  /// Start a vm
  Start {
    /// Name of the vm
    name: String,
  },
  /// Stop a vm
  Stop {
    /// Name of the vm
    name: String,
  },
  /// Attach to a vm
  Attach {
    /// Name of the vm
    name: String,
  },
  /// Patch a vm
  Patch(VmPatchOpts),
}

#[derive(Clone, Debug, Parser)]
pub struct VmPatchOpts {
  /// Name of the vm
  pub name: String,
  /// Default user of the VM
  #[clap(long)]
  pub user: Option<String>,
  /// Default password of the VM
  #[clap(long)]
  pub password: Option<String>,
  /// Ssh key for the user
  #[clap(long)]
  pub ssh_key: Option<String>,
  /// hostname of the vm
  #[clap(long)]
  pub hostname: Option<String>,
  /// Cpu of the vm default to 1
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory of the vm in MB default to 512
  #[clap(long)]
  pub memory: Option<u64>,
  /// network interface of the vm
  #[clap(long)]
  pub net_iface: Option<String>,
}

impl From<VmPatchOpts> for VmConfigUpdate {
  fn from(val: VmPatchOpts) -> Self {
    Self {
      name: Some(val.name),
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      hostname: val.hostname,
      host_config: Some(VmHostConfig {
        cpu: val.cpu.unwrap_or(1),
        memory: val.memory.unwrap_or(512),
        net_iface: val.net_iface,
        ..Default::default()
      }),
      ..Default::default()
    }
  }
}

#[derive(Clone, Debug, Parser)]
pub struct VmRunOpts {
  /// hostname of the vm
  #[clap(long)]
  pub hostname: Option<String>,
  /// Cpu of the vm default to 1
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory of the vm in MB default to 512
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// network interface of the vm
  #[clap(long)]
  pub net_iface: Option<String>,
  /// Default user of the VM
  #[clap(long)]
  pub user: Option<String>,
  /// Default password of the VM
  #[clap(long)]
  pub password: Option<String>,
  /// Ssh key for the user
  #[clap(long)]
  pub ssh_key: Option<String>,
  /// Name of the vm
  pub name: String,
  /// Name of the vm image
  pub image: String,
}

impl From<VmRunOpts> for VmConfigPartial {
  fn from(val: VmRunOpts) -> Self {
    Self {
      name: val.name,
      hostname: val.hostname,
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      disk: VmDiskConfig {
        image: val.image,
        ..Default::default()
      },
      host_config: VmHostConfig {
        cpu: val.cpu.unwrap_or(1),
        memory: val.memory.unwrap_or(512),
        net_iface: val.net_iface,
        ..Default::default()
      },
      ..Default::default()
    }
  }
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
      disk: VmDiskConfig {
        image: val.image,
        ..Default::default()
      },
      ..Default::default()
    }
  }
}

#[derive(Tabled)]
pub struct VmRow {
  pub(crate) name: String,
  pub(crate) namespace: String,
  pub(crate) disk: String,
  pub(crate) instances: String,
  pub(crate) config_version: String,
  pub(crate) created_at: String,
  pub(crate) updated_at: String,
}

impl From<VmSummary> for VmRow {
  fn from(vm: VmSummary) -> Self {
    // Convert the created_at and updated_at to the current timezone
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(vm.created_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(vm.updated_at.timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      name: vm.name,
      namespace: vm.namespace_name,
      disk: vm.config.disk.image,
      config_version: vm.config.version,
      instances: format!("{}/{}", vm.running_instances, vm.instances),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
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
