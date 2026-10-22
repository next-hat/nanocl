use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::vm::VmSummary;
use nanocld_client::stubs::vm_spec::{
  VmHostConfig, VmHostConfigUpdate, VmSpecPartial, VmSpecUpdate,
};

use super::{
  GenericInspectOpts, GenericRemoveOpts, GenericStartOpts, NamespacedListOpts,
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
  /// Shut down VMs by canonical keys
  Stop(VmLifecycleOpts),
  /// Shut down and start VMs by canonical keys
  Restart(VmLifecycleOpts),
  /// Query actual QEMU execution state
  Status {
    /// Canonical key of the VM
    key: String,
  },
  /// Grow CPU, memory, or boot disk without restarting the VM
  Resize(VmResizeOpts),
  /// Query QEMU resources and whether CPU/memory persist across restart
  Resources {
    /// Canonical key of the VM
    key: String,
  },
  /// Attach to a vm
  Attach {
    /// Canonical key of the VM
    key: String,
  },
  /// Patch a vm
  Patch(VmPatchOpts),
}

/// Options for VM shutdown and restart.
#[derive(Clone, Parser)]
pub struct VmLifecycleOpts {
  /// Canonical keys of the VMs
  #[clap(required = true)]
  pub keys: Vec<String>,
  /// Seconds to wait for guest shutdown; a timeout never forces power-off
  #[clap(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=3600))]
  pub timeout: u64,
  /// Immediately power off instead of asking the guest to shut down
  #[clap(short, long)]
  pub force: bool,
}

impl From<&VmLifecycleOpts> for nanocld_client::stubs::vm::VmLifecycleOptions {
  fn from(value: &VmLifecycleOpts) -> Self {
    Self {
      timeout: value.timeout,
      force: value.force,
    }
  }
}

/// Requested resource totals for a live VM resize.
#[derive(Clone, Parser)]
#[clap(group(clap::ArgGroup::new("resources").required(true).multiple(true).args(["cpu", "memory", "disk_size"])))]
pub struct VmResizeOpts {
  /// Canonical key of the VM
  pub key: String,
  /// Total vCPU count, up to the boot-time MaxCpu
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub cpu: Option<u64>,
  /// Total memory in MiB, up to the boot-time MaxMemory
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub memory: Option<u64>,
  /// Total boot disk size in bytes; does not grow guest partitions/filesystems
  #[clap(long, value_name = "BYTES", value_parser = clap::value_parser!(u64).range(1..))]
  pub disk_size: Option<u64>,
}

impl From<&VmResizeOpts> for nanocld_client::stubs::vm::VmResizeOptions {
  fn from(value: &VmResizeOpts) -> Self {
    Self {
      cpu: value.cpu,
      memory: value.memory,
      disk_size: value.disk_size,
    }
  }
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
  /// Read complete cloud-init user-data from a UTF-8 file (maximum 64 KiB)
  #[clap(long, value_name = "FILE")]
  pub cloud_init_user_data: Option<String>,
  /// Restore generated User/Password/SshKey provisioning
  #[clap(long, conflicts_with = "cloud_init_user_data")]
  pub clear_cloud_init_user_data: bool,
  /// Enable the authenticated browser console (true or false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
  /// Number of CPUs; omitted preserves the current setting
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory in MB; omitted preserves the current setting
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// Maximum vCPU count reserved at boot; changing it replaces the runtime
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_cpu: Option<u64>,
  /// Maximum memory in MiB reserved at boot; changing it replaces the runtime
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_memory: Option<u64>,
  /// Enable KVM
  #[clap(long)]
  pub kvm: bool,
  /// network interface of the vm
  #[clap(long)]
  pub net_iface: Option<String>,
}

impl VmPatchOpts {
  /// Send only explicitly supplied host settings, merged atomically by the daemon.
  pub(crate) fn patch_host_config(&self) -> Option<VmHostConfigUpdate> {
    if self.cpu.is_none()
      && self.memory.is_none()
      && self.max_cpu.is_none()
      && self.max_memory.is_none()
      && self.net_iface.is_none()
      && !self.kvm
    {
      return None;
    }
    Some(VmHostConfigUpdate {
      cpu: self.cpu,
      memory: self.memory,
      max_cpu: self.max_cpu.map(Some),
      max_memory: self.max_memory.map(Some),
      net_iface: self.net_iface.clone().map(Some),
      kvm: self.kvm.then_some(Some(true)),
      ..Default::default()
    })
  }
}

/// Convert VmPatchOpts to VmSpecUpdate
impl From<VmPatchOpts> for VmSpecUpdate {
  fn from(val: VmPatchOpts) -> Self {
    let host_config = val.patch_host_config();
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
  /// Read complete cloud-init user-data from a UTF-8 file (maximum 64 KiB)
  #[clap(long, value_name = "FILE")]
  pub cloud_init_user_data: Option<String>,
  /// Enable the authenticated browser console (default: false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
  /// Cpu of the vm default to 1
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory of the vm in MB default to 512
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// Maximum vCPU count reserved at boot; defaults to the initial CPU count
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_cpu: Option<u64>,
  /// Maximum memory in MiB reserved at boot; defaults to the initial memory
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_memory: Option<u64>,
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
        max_cpu: val.max_cpu,
        max_memory: val.max_memory,
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
  /// Read complete cloud-init user-data from a UTF-8 file (maximum 64 KiB)
  #[clap(long, value_name = "FILE")]
  pub cloud_init_user_data: Option<String>,
  /// Enable the authenticated browser console (default: false)
  #[clap(long, action = clap::ArgAction::Set)]
  pub console: Option<bool>,
  /// Cpu of the vm default to 1
  #[clap(long)]
  pub cpu: Option<u64>,
  /// Memory of the vm in MB default to 512
  #[clap(long = "mem")]
  pub memory: Option<u64>,
  /// Maximum vCPU count reserved at boot; defaults to the initial CPU count
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_cpu: Option<u64>,
  /// Maximum memory in MiB reserved at boot; defaults to the initial memory
  #[clap(long, value_parser = clap::value_parser!(u64).range(1..))]
  pub max_memory: Option<u64>,
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
        max_cpu: val.max_cpu,
        max_memory: val.max_memory,
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
  fn vm_cli_resize_requires_positive_totals_and_accepts_multiple_resources() {
    assert!(VmArg::try_parse_from(["vm", "resize", "global.guest"]).is_err());
    for flag in ["--cpu", "--memory", "--disk-size"] {
      assert!(
        VmArg::try_parse_from(["vm", "resize", "global.guest", flag, "0"])
          .is_err()
      );
    }
    let args = VmArg::try_parse_from([
      "vm",
      "resize",
      "global.guest",
      "--cpu",
      "4",
      "--memory",
      "2048",
      "--disk-size",
      "8589934592",
    ])
    .unwrap();
    let VmCommand::Resize(options) = args.command else {
      panic!("resize");
    };
    let request = nanocld_client::stubs::vm::VmResizeOptions::from(&options);
    assert_eq!(request.cpu, Some(4));
    assert_eq!(request.memory, Some(2048));
    assert_eq!(request.disk_size, Some(8589934592));
    assert!(
      matches!(VmArg::try_parse_from(["vm", "resources", "global.guest"]).unwrap().command, VmCommand::Resources { key } if key == "global.guest")
    );
  }

  #[test]
  fn vm_cli_boot_limits_round_trip_and_patch_preserves_omitted_limits() {
    for action in ["create", "run"] {
      let args = VmArg::try_parse_from([
        "vm",
        action,
        "guest",
        "/vm.qcow2",
        "--max-cpu",
        "8",
        "--max-memory",
        "8192",
      ])
      .unwrap();
      let spec = match args.command {
        VmCommand::Create(options) => VmSpecPartial::from(options),
        VmCommand::Run(options) => VmSpecPartial::from(options),
        _ => panic!("create or run"),
      };
      let host = spec.host_config.unwrap();
      assert_eq!(host.max_cpu, Some(8));
      assert_eq!(host.max_memory, Some(8192));
    }
    let current = VmHostConfig {
      cpu: 2,
      memory: 1024,
      max_cpu: Some(8),
      max_memory: Some(8192),
      ..Default::default()
    };
    let options =
      VmPatchOpts::try_parse_from(["patch", "global.guest", "--max-cpu", "16"])
        .unwrap();
    let updated = options.patch_host_config().unwrap().apply_to(&current);
    assert_eq!(updated.max_cpu, Some(16));
    assert_eq!(updated.max_memory, current.max_memory);
    assert_eq!(updated.cpu, current.cpu);
    assert_eq!(updated.memory, current.memory);
    let options =
      VmPatchOpts::try_parse_from(["patch", "global.guest", "--cpu", "4"])
        .unwrap();
    assert_eq!(
      options
        .patch_host_config()
        .unwrap()
        .apply_to(&current)
        .max_cpu,
      current.max_cpu
    );
  }

  #[test]
  fn vm_cli_lifecycle_defaults_force_and_timeout_bounds() {
    for action in ["stop", "restart"] {
      let parsed =
        VmArg::try_parse_from(["vm", action, "global.guest"]).unwrap();
      let options = match parsed.command {
        VmCommand::Stop(options) | VmCommand::Restart(options) => options,
        _ => panic!("expected lifecycle command"),
      };
      assert_eq!(options.timeout, 60);
      assert!(!options.force);
      let parsed = VmArg::try_parse_from([
        "vm",
        action,
        "global.guest",
        "team.other",
        "--timeout",
        "3600",
        "--force",
      ])
      .unwrap();
      let options = match parsed.command {
        VmCommand::Stop(options) | VmCommand::Restart(options) => options,
        _ => panic!("expected lifecycle command"),
      };
      assert_eq!(options.keys, ["global.guest", "team.other"]);
      assert_eq!(options.timeout, 3600);
      assert!(options.force);
      for timeout in ["0", "3601", "-1"] {
        assert!(
          VmArg::try_parse_from([
            "vm",
            action,
            "global.guest",
            "--timeout",
            timeout
          ])
          .is_err()
        );
      }
      assert!(VmArg::try_parse_from(["vm", action]).is_err());
    }
    assert!(
      matches!(VmArg::try_parse_from(["vm", "status", "global.guest"]).unwrap().command, VmCommand::Status { key } if key == "global.guest")
    );
  }

  #[test]
  fn vm_cli_custom_cloud_init_accepts_file_for_create_run_and_patch() {
    for action in ["create", "run"] {
      let args = VmArg::try_parse_from([
        "vm",
        action,
        "guest",
        "/vm.qcow2",
        "--cloud-init-user-data",
        "./user data.yaml",
      ])
      .unwrap();
      let file = match args.command {
        VmCommand::Create(opts) => opts.cloud_init_user_data,
        VmCommand::Run(opts) => opts.cloud_init_user_data,
        _ => panic!("expected create or run"),
      };
      assert_eq!(file.as_deref(), Some("./user data.yaml"));
    }
    let args = VmArg::try_parse_from([
      "vm",
      "patch",
      "global.guest",
      "--cloud-init-user-data",
      "./replacement.yaml",
    ])
    .unwrap();
    let VmCommand::Patch(opts) = args.command else {
      panic!("expected patch");
    };
    assert_eq!(
      opts.cloud_init_user_data.as_deref(),
      Some("./replacement.yaml")
    );
    assert!(opts.patch_host_config().is_none());
    let omitted =
      VmPatchOpts::try_parse_from(["patch", "global.guest"]).unwrap();
    assert!(omitted.cloud_init_user_data.is_none());
    assert!(
      VmPatchOpts::try_parse_from([
        "patch",
        "global.guest",
        "--cloud-init-user-data"
      ])
      .is_err()
    );
  }

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

  #[test]
  fn vm_cli_cloud_init_clear_is_explicit_and_conflicts_with_replacement() {
    let options = VmPatchOpts::try_parse_from([
      "patch",
      "global.guest",
      "--clear-cloud-init-user-data",
    ])
    .unwrap();
    assert!(options.clear_cloud_init_user_data);
    assert!(options.cloud_init_user_data.is_none());
    assert!(
      !VmPatchOpts::try_parse_from(["patch", "global.guest"])
        .unwrap()
        .clear_cloud_init_user_data
    );
    assert!(
      VmPatchOpts::try_parse_from([
        "patch",
        "global.guest",
        "--clear-cloud-init-user-data",
        "--cloud-init-user-data",
        "./user-data.yaml",
      ])
      .is_err()
    );
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
      let patch = options.patch_host_config().unwrap();
      assert_eq!(patch.apply_to(&current), expected);
      let serialized = serde_json::to_value(patch).unwrap();
      assert_eq!(serialized.as_object().unwrap().len(), 1);
    }
    let options =
      VmPatchOpts::try_parse_from(["patch", "global.guest", "--kvm"]).unwrap();
    let mut without_kvm = current.clone();
    without_kvm.kvm = None;
    assert_eq!(
      options.patch_host_config().unwrap().apply_to(&without_kvm),
      current
    );
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
