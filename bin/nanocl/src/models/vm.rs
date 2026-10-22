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
  /// Enable cloud-init provisioning (true or false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub cloud_init: Option<bool>,
  /// Enable the authenticated browser console (true or false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
  /// Number of CPUs; omitted preserves the current setting
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory in MB; omitted preserves the current setting
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// Enable KVM
  #[clap(long)]
  pub kvm: bool,
  /// network interface of the vm
  #[clap(long)]
  pub net_iface: Option<String>,
}

impl VmPatchOpts {
  /// Overlay only explicitly supplied host settings. Console/provisioning
  /// patches must not replace the runtime image, network, or resource limits.
  pub(crate) fn patch_host_config(
    &self,
    current: &VmHostConfig,
  ) -> Option<VmHostConfig> {
    if self.cpu.is_none()
      && self.memory.is_none()
      && self.net_iface.is_none()
      && !self.kvm
    {
      return None;
    }
    let mut config = current.clone();
    if let Some(cpu) = self.cpu {
      config.cpu = cpu;
    }
    if let Some(memory) = self.memory {
      config.memory = memory;
    }
    if let Some(interface) = &self.net_iface {
      config.net_iface = Some(interface.clone());
    }
    if self.kvm {
      config.kvm = Some(true);
    }
    Some(config)
  }
}

/// Convert VmPatchOpts to VmSpecUpdate
impl From<VmPatchOpts> for VmSpecUpdate {
  fn from(val: VmPatchOpts) -> Self {
    let host_config = val.patch_host_config(&VmHostConfig::default());
    Self {
      name: None,
      user: val.user,
      password: val.password,
      ssh_key: val.ssh_key,
      hostname: val.hostname,
      cloud_init: val.cloud_init,
      console: val.console,
      host_config,
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
  /// Enable cloud-init provisioning (default: true)
  #[clap(long, action = clap::ArgAction::Set)]
  pub cloud_init: Option<bool>,
  /// Enable the authenticated browser console (default: false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
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
      cloud_init: val.cloud_init,
      console: val.console,
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
  /// Enable cloud-init provisioning (default: true)
  #[clap(long, action = clap::ArgAction::Set)]
  pub cloud_init: Option<bool>,
  /// Enable the authenticated browser console (default: false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
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
      cloud_init: val.cloud_init,
      console: val.console,
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
  /// Age since the VM was created, used in compact output
  #[tabled(skip)]
  pub(crate) age: String,
}

/// A compact row for the VM table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct VmCompactRow {
  pub(crate) key: String,
  pub(crate) image: String,
  pub(crate) status: String,
  pub(crate) instances: String,
  pub(crate) age: String,
}

impl From<VmRow> for VmCompactRow {
  fn from(row: VmRow) -> Self {
    Self {
      key: row.key,
      image: row.image,
      status: row.status,
      instances: row.instances,
      age: row.age,
    }
  }
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
      age: super::format_age(
        Some(&vm.created_at.and_utc()),
        chrono::Utc::now(),
      ),
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
  use nanocld_client::stubs::{system::ObjPsStatus, vm_spec::VmSpec};

  #[test]
  fn vm_cli_optional_provisioning_and_console_accept_explicit_booleans() {
    for action in ["create", "run"] {
      let args = VmArg::try_parse_from([
        "vm",
        action,
        "guest",
        "/vm.qcow2",
        "--cloud-init",
        "false",
        "--console",
        "true",
      ])
      .unwrap();
      let spec = match args.command {
        VmCommand::Create(opts) => VmSpecPartial::from(opts),
        VmCommand::Run(opts) => VmSpecPartial::from(opts),
        _ => panic!("expected create or run"),
      };
      assert_eq!(spec.cloud_init, Some(false));
      assert_eq!(spec.console, Some(true));
    }
    let args = VmArg::try_parse_from([
      "vm",
      "patch",
      "global.guest",
      "--console",
      "false",
    ])
    .unwrap();
    let VmCommand::Patch(opts) = args.command else {
      panic!("expected patch");
    };
    let update = VmSpecUpdate::from(opts);
    assert_eq!(update.console, Some(false));
    assert_eq!(update.cloud_init, None);
    assert!(update.host_config.is_none());
  }

  use super::*;

  #[test]
  fn vm_cli_host_patch_preserves_runtime_network_and_unrequested_resources() {
    let current = VmHostConfig {
      cpu: 8,
      memory: 4096,
      kvm: Some(true),
      runtime: Some("local/runtime:candidate".to_owned()),
      runtime_network: Some("private-vms".to_owned()),
      dns: Some(vec!["192.0.2.53".to_owned()]),
      link_net_iface: Some("eth1".to_owned()),
      ..Default::default()
    };
    for flag in ["--cpu", "--mem", "--net-iface"] {
      let value = if flag == "--net-iface" { "ens4" } else { "2" };
      let options =
        VmPatchOpts::try_parse_from(["patch", "global.guest", flag, value])
          .unwrap();
      let mut expected = current.clone();
      match flag {
        "--cpu" => expected.cpu = 2,
        "--mem" => expected.memory = 2,
        _ => expected.net_iface = Some("ens4".to_owned()),
      }
      assert_eq!(options.patch_host_config(&current), Some(expected));
    }
    let options =
      VmPatchOpts::try_parse_from(["patch", "global.guest", "--kvm"]).unwrap();
    let mut without_kvm = current.clone();
    without_kvm.kvm = None;
    assert_eq!(options.patch_host_config(&without_kvm), Some(current));
  }

  #[test]
  fn compact_vm_row_preserves_image_status_and_age() {
    let created_at =
      (chrono::Utc::now() - chrono::Duration::hours(49)).naive_utc();
    let status = ObjPsStatus::default();
    let expected_status = format!("{}/{}", status.actual, status.wanted);
    let row = VmRow::from(VmSummary {
      namespace_name: "production".to_owned(),
      status,
      created_at,
      instance_total: 2,
      instance_running: 1,
      spec: VmSpec {
        vm_key: "production.database".to_owned(),
        image: "/var/lib/nanocl/vm/database.qcow2".to_owned(),
        version: "v1".to_owned(),
        created_at: chrono::Utc::now().naive_utc(),
        ..Default::default()
      },
    });
    assert_eq!(row.age, "2d");
    assert_eq!(row.version, "v1");
    assert!(!row.created_at.is_empty());
    assert!(!row.updated_at.is_empty());
    assert_eq!(
      VmRow::headers(),
      [
        "KEY",
        "IMAGE",
        "STATUS",
        "INSTANCES",
        "VERSION",
        "CREATED AT",
        "UPDATED AT"
      ]
    );
    let compact = VmCompactRow::from(row);
    assert_eq!(compact.key, "production.database");
    assert_eq!(compact.image, "/var/lib/nanocl/vm/database.qcow2");
    assert_eq!(compact.status, expected_status);
    assert_eq!(compact.instances, "1/2");
    assert_eq!(compact.age, "2d");
    assert_eq!(
      VmCompactRow::headers(),
      ["KEY", "IMAGE", "STATUS", "INSTANCES", "AGE"]
    );
  }

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
        age: String::new(),
      },
      VmRow {
        key: "production.same".to_owned(),
        image: "image.qcow2".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
        age: String::new(),
      },
    ])
    .to_string();
    assert!(table.contains("KEY"));
    assert!(table.contains("global.same"));
    assert!(table.contains("production.same"));
  }
}
