use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::vm::VmSummary;
use nanocld_client::stubs::vm_spec::{
  VmHostConfig, VmSpecPartial, VmSpecUpdate,
};

use super::{
  GenericInspectOpts, GenericRemoveOpts, GenericStartOpts, GenericStopOpts,
  NamespacedListOpts,
};

/// `nanocl vm` available commands
#[derive(Clone, Subcommand)]
pub enum VmCommand {
  /// Run a vm
  Run(VmRunOpts),
  /// Create a vm
  Create(VmCreateOpts),
  /// List vms
  #[clap(alias = "ls")]
  List(NamespacedListOpts),
  /// Remove VMs by canonical keys
  #[clap(alias = "rm")]
  Remove(GenericRemoveOpts),
  /// Inspect a VM by canonical key
  Inspect(GenericInspectOpts),
  /// Start VMs by canonical keys
  Start(GenericStartOpts),
  /// Stop VMs by canonical keys
  Stop(GenericStopOpts),
  /// Attach to a vm
  Attach {
    /// Canonical key of the VM
    key: String,
  },
  /// Patch a vm
  Patch(VmPatchOpts),
}

/// `nanocl vm patch` available options
#[derive(Clone, Parser)]
pub struct VmPatchOpts {
  /// Canonical key of the VM
  pub key: String,
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
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// Enable KVM
  #[clap(long)]
  pub kvm: bool,
  /// network interface of the vm
  #[clap(long)]
  pub net_iface: Option<String>,
}

/// Convert VmPatchOpts to VmSpecUpdate
impl From<VmPatchOpts> for VmSpecUpdate {
  fn from(val: VmPatchOpts) -> Self {
    Self {
      name: None,
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      hostname: val.hostname,
      host_config: Some(VmHostConfig {
        kvm: Some(val.kvm),
        cpu: val.cpu.unwrap_or(1),
        memory: val.memory.unwrap_or(512),
        net_iface: val.net_iface,
        ..Default::default()
      }),
      ..Default::default()
    }
  }
}

/// `nanocl vm run` available options
#[derive(Clone, Parser)]
pub struct VmRunOpts {
  /// Namespace for the new VM; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
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
  /// Enable KVM
  #[clap(long)]
  pub kvm: bool,
  /// Attach to the vm
  #[clap(short, long)]
  pub attach: bool,
  /// Name of the vm
  pub name: String,
  /// Full path of the vm image
  pub image: String,
}

/// Convert VmRunOpts to VmSpecPartial
impl From<VmRunOpts> for VmSpecPartial {
  fn from(val: VmRunOpts) -> Self {
    Self {
      name: val.name,
      hostname: val.hostname,
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      image: val.image,
      host_config: Some(VmHostConfig {
        cpu: val.cpu.unwrap_or(1),
        memory: val.memory.unwrap_or(512),
        net_iface: val.net_iface,
        kvm: Some(val.kvm),
        ..Default::default()
      }),
      ..Default::default()
    }
  }
}

/// `nanocl vm create` available options
#[derive(Clone, Parser)]
pub struct VmCreateOpts {
  /// Namespace for the new VM; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
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
  /// Enable KVM
  #[clap(long)]
  pub kvm: bool,
  /// Name of the vm
  pub name: String,
  /// Full path of the vm image
  pub image: String,
}

/// Convert VmCreateOpts to VmSpecPartial
impl From<VmCreateOpts> for VmSpecPartial {
  fn from(val: VmCreateOpts) -> Self {
    Self {
      name: val.name,
      hostname: val.hostname,
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      host_config: Some(VmHostConfig {
        cpu: val.cpu.unwrap_or(1),
        memory: val.memory.unwrap_or(512),
        net_iface: val.net_iface,
        kvm: Some(val.kvm),
        ..Default::default()
      }),
      image: val.image,
      ..Default::default()
    }
  }
}

/// A row for the vm table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct VmRow {
  /// Canonical key of the VM
  pub(crate) key: String,
  /// Image of the vm
  pub(crate) image: String,
  /// Status of the vm
  pub(crate) status: String,
  /// Number of instances
  pub(crate) instances: String,
  /// Spec version
  pub(crate) version: String,
  /// When the vm was created
  #[tabled(rename = "CREATED AT")]
  pub(crate) created_at: String,
  /// When the vm was last updated
  #[tabled(rename = "UPDATED AT")]
  pub(crate) updated_at: String,
}

/// Convert VmSummary to VmRow
impl From<VmSummary> for VmRow {
  fn from(vm: VmSummary) -> Self {
    // Convert the created_at and updated_at to the current timezone
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(vm.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(vm.spec.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: vm.spec.vm_key,
      image: vm.spec.image,
      version: vm.spec.version,
      status: format!("{}/{}", vm.status.actual, vm.status.wanted),
      instances: format!("{}/{}", vm.instance_running, vm.instance_total),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

/// `nanocl vm` available arguments
#[derive(Clone, Parser)]
pub struct VmArg {
  /// subcommand to run
  #[clap(subcommand)]
  pub command: VmCommand,
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  #[test]
  fn namespace_is_only_available_for_collection_and_creation_commands() {
    let inspect =
      VmArg::try_parse_from(["vm", "inspect", "production.database"]).unwrap();
    assert!(matches!(inspect.command, VmCommand::Inspect(_)));
    assert!(
      VmArg::try_parse_from([
        "vm",
        "--namespace",
        "production",
        "inspect",
        "production.database"
      ])
      .is_err()
    );

    let list =
      VmArg::try_parse_from(["vm", "list", "--namespace", "production"])
        .unwrap();
    let VmCommand::List(list) = list.command else {
      panic!("expected list command");
    };
    assert_eq!(list.namespace.as_deref(), Some("production"));
  }

  #[test]
  fn table_rows_distinguish_duplicate_local_names() {
    let table = tabled::Table::new([
      VmRow {
        key: "global.same".to_owned(),
        image: "image.qcow2".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
      VmRow {
        key: "production.same".to_owned(),
        image: "image.qcow2".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
    ])
    .to_string();
    assert!(table.contains("KEY"));
    assert!(table.contains("global.same"));
    assert!(table.contains("production.same"));
  }
}
