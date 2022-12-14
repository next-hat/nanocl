use std::hash::Hash;
use std::collections::HashMap;

use tabled::Tabled;
use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

use super::cargo_image::CargoImageArgs;
use super::cargo_instance::{CargoInstanceArgs, CargoInstanceSummary};

/// PortMap describes the mapping of container ports to host ports, using the container's port-number and protocol as key in the format `<port>/<protocol>`, for example, `80/udp`.  If a container's port is mapped for multiple protocols, separate entries are added to the mapping table.
// special-casing PortMap, cos swagger-codegen doesn't figure out this type
pub type PortMap = HashMap<String, Option<Vec<PortBinding>>>;

/// Cargo delete options
#[derive(Debug, Parser)]
pub struct CargoDeleteOptions {
  /// Name of cargo to delete
  pub name: String,
}

/// Cargo start options
#[derive(Debug, Parser)]
pub struct CargoStartOptions {
  // Name of cargo to start
  pub name: String,
}

#[derive(Debug, Parser)]
pub struct CargoInspectOption {
  /// Name of cargo to inspect
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum CargoPatchCommands {
  Set(CargoPatchPartial),
}

#[derive(Debug, Parser)]
pub struct CargoPatchArgs {
  pub(crate) name: String,
  #[clap(subcommand)]
  pub(crate) commands: CargoPatchCommands,
}

#[derive(Debug, Subcommand)]
#[clap(about, version)]
pub enum CargoCommands {
  /// List existing cargo
  #[clap(alias("ls"))]
  List,
  /// Create a new cargo
  Create(NewCargo),
  /// Remove cargo by it's name
  #[clap(alias("rm"))]
  Remove(CargoDeleteOptions),
  /// Inspect a cargo by it's name
  Inspect(CargoInspectOption),
  /// Update a cargo by it's name
  Patch(CargoPatchArgs),
  /// Manage cargo instances
  Instance(CargoInstanceArgs),
  /// Manage cargo image
  Image(CargoImageArgs),
}

/// Manage cargoes
#[derive(Debug, Parser)]
#[clap(name = "nanocl-cargo")]
pub struct CargoArgs {
  /// namespace to target by default global is used
  #[clap(long)]
  pub namespace: Option<String>,
  #[clap(subcommand)]
  pub commands: CargoCommands,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct NewCargo {
  /// Name of the new cargo
  pub(crate) name: String,
  /// Image used by the cargo
  #[clap(long)]
  pub(crate) image: String,
  /// Optional domain to bind to in format ip:domain.com
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  /// Environment variables used by the cargo
  #[clap(long = "env")]
  pub(crate) environnements: Option<Vec<String>>,
  /// Number of replicas default to 1
  #[clap(long)]
  pub(crate) replicas: Option<i32>,
}

// impl From<NewCargo> for CargoPartial {
//   fn from(cargo: NewCargo) -> Self {
//     Self {
//       name: cargo.name,
//       dns_entry: cargo.dns_entry,
//       environnements: cargo.environnements,
//       replicas: cargo.replicas,
//       config: CargoConfig::default(),
//     }
//   }
// }

/// Convert NewCargo into CargoPartial
impl From<&NewCargo> for CargoPartial {
  fn from(cargo: &NewCargo) -> Self {
    Self {
      name: cargo.name.to_owned(),
      dns_entry: cargo.dns_entry.to_owned(),
      environnements: cargo.environnements.to_owned(),
      replicas: cargo.replicas,
      config: CargoConfig {
        image: Some(cargo.image.to_owned()),
        ..Default::default()
      },
    }
  }
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct CargoPartial {
  /// Name of the cargo
  pub(crate) name: String,
  // #[clap(long)]
  // pub(crate) image: String,
  /// Optional domain to bind to in format ip:domain.com
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  /// Environment variables to set
  #[clap(long = "env")]
  pub(crate) environnements: Option<Vec<String>>,
  /// Number of replicas default to 1
  #[clap(long)]
  pub(crate) replicas: Option<i32>,
  /// Global container config
  #[clap(skip)]
  pub(crate) config: CargoConfig,
}

/// Cargo item is an definition to container create image and start them
/// this structure ensure read and write in database
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoItem {
  pub(crate) key: String,
  pub(crate) name: String,
  pub(crate) replicas: i32,
  // #[serde(rename = "network_name")]
  // pub(crate) network: Option<String>,
  #[tabled(skip)]
  pub(crate) config: CargoConfig,
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoEnvItem {
  #[tabled(skip)]
  pub(crate) key: String,
  #[tabled(skip)]
  pub(crate) cargo_key: String,
  pub(crate) name: String,
  pub(crate) value: String,
}

/// Cargo item with his relation
#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct CargoItemWithRelation {
  pub(crate) key: String,
  #[tabled(skip)]
  pub(crate) namespace_name: String,
  pub(crate) name: String,
  pub(crate) replicas: i32,
  #[tabled(skip)]
  pub(crate) containers: Vec<CargoInstanceSummary>,
  #[tabled(skip)]
  pub(crate) environnements: Option<Vec<CargoEnvItem>>,
  #[tabled(skip)]
  pub(crate) config: CargoConfig,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct CargoPatchPartial {
  #[clap(long)]
  pub(crate) name: Option<String>,
  #[clap(long = "image")]
  pub(crate) image_name: Option<String>,
  #[clap(long = "bind")]
  pub(crate) binds: Option<Vec<String>>,
  #[clap(long)]
  pub(crate) replicas: Option<i32>,
  #[clap(long)]
  pub(crate) dns_entry: Option<String>,
  #[clap(long)]
  pub(crate) domainname: Option<String>,
  #[clap(long)]
  pub(crate) hostname: Option<String>,
  #[clap(long = "env")]
  pub(crate) environnements: Option<Vec<String>>,
}

/// Container to create.
#[derive(Debug, Clone, Parser, Default, PartialEq, Serialize, Deserialize)]
pub struct CargoConfig {
  /// The hostname to use for the container, as a valid RFC 1123 hostname.
  #[serde(rename = "Hostname")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hostname: Option<String>,

  /// The domain name to use for the container.
  #[serde(rename = "Domainname")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub domainname: Option<String>,

  /// The user that commands are run as inside the container.
  #[serde(rename = "User")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user: Option<String>,

  /// Whether to attach to `stdin`.
  #[serde(rename = "AttachStdin")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stdin: Option<bool>,

  /// Whether to attach to `stdout`.
  #[serde(rename = "AttachStdout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stdout: Option<bool>,

  /// Whether to attach to `stderr`.
  #[serde(rename = "AttachStderr")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stderr: Option<bool>,

  /// An object mapping ports to an empty object in the form:  `{\"<port>/<tcp|udp|sctp>\": {}}`
  #[clap(skip)]
  #[serde(rename = "ExposedPorts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exposed_ports: Option<HashMap<String, HashMap<(), ()>>>,

  /// Attach standard streams to a TTY, including `stdin` if it is not closed.
  #[serde(rename = "Tty")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tty: Option<bool>,

  /// Open `stdin`
  #[serde(rename = "OpenStdin")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub open_stdin: Option<bool>,

  /// Close `stdin` after one attached client disconnects
  #[serde(rename = "StdinOnce")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stdin_once: Option<bool>,

  /// A list of environment variables to set inside the container in the form `[\"VAR=value\", ...]`. A variable without `=` is removed from the environment, rather than to have an empty value.
  #[serde(rename = "Env")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub env: Option<Vec<String>>,

  /// Command to run specified as a string or an array of strings.
  #[serde(rename = "Cmd")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cmd: Option<Vec<String>>,

  /// A TEST to perform TO Check that the container is healthy.
  #[clap(skip)]
  #[serde(rename = "Healthcheck")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub healthcheck: Option<HealthConfig>,

  /// Command is already escaped (Windows only)
  #[serde(rename = "ArgsEscaped")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub args_escaped: Option<bool>,

  /// The name of the image to use when creating the container
  #[serde(rename = "Image")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image: Option<String>,

  /// An object mapping mount point paths inside the container to empty objects.
  #[clap(skip)]
  #[serde(rename = "Volumes")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volumes: Option<HashMap<String, HashMap<(), ()>>>,

  /// The working directory for commands to run in.
  #[serde(rename = "WorkingDir")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub working_dir: Option<String>,

  /// The entry point for the container as a string or an array of strings.  If the array consists of exactly one empty string (`[\"\"]`) then the entry point is reset to system default (i.e., the entry point used by docker when there is no `ENTRYPOINT` instruction in the `Dockerfile`).
  #[serde(rename = "Entrypoint")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub entrypoint: Option<Vec<String>>,

  /// Disable networking for the container.
  #[serde(rename = "NetworkDisabled")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub network_disabled: Option<bool>,

  /// MAC address of the container.
  #[serde(rename = "MacAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mac_address: Option<String>,

  /// `ONBUILD` metadata that were defined in the image's `Dockerfile`.
  #[serde(rename = "OnBuild")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub on_build: Option<Vec<String>>,

  /// User-defined key/value metadata.
  #[clap(skip)]
  #[serde(rename = "Labels")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub labels: Option<HashMap<String, String>>,

  /// Signal to stop a container as a string or unsigned integer.
  #[serde(rename = "StopSignal")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stop_signal: Option<String>,

  /// Timeout to stop a container in seconds.
  #[serde(rename = "StopTimeout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stop_timeout: Option<i64>,

  /// Shell for when `RUN`, `CMD`, and `ENTRYPOINT` uses a shell.
  #[serde(rename = "Shell")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub shell: Option<Vec<String>>,

  /// Container configuration that depends on the host we are running on.
  /// Shell for when `RUN`, `CMD`, and `ENTRYPOINT` uses a shell.
  #[clap(skip)]
  #[serde(rename = "HostConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub host_config: Option<HostConfig>,

  /// This container's networking configuration.
  #[clap(skip)]
  #[serde(rename = "NetworkingConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub networking_config: Option<NetworkingConfig<String>>,
}

/// A test to perform to check that the container is healthy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HealthConfig {
  /// The test to perform. Possible values are:  - `[]` inherit healthcheck from image or parent image - `[\"NONE\"]` disable healthcheck - `[\"CMD\", args...]` exec arguments directly - `[\"CMD-SHELL\", command]` run command with system's default shell
  #[serde(rename = "Test")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub test: Option<Vec<String>>,

  /// The time to wait between checks in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "Interval")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub interval: Option<i64>,

  /// The time to wait before considering the check to have hung. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "Timeout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timeout: Option<i64>,

  /// The number of consecutive failures needed to consider a container as unhealthy. 0 means inherit.
  #[serde(rename = "Retries")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub retries: Option<i64>,

  /// Start period for the container to initialize before starting health-retries countdown in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "StartPeriod")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub start_period: Option<i64>,
}

/// Container configuration that depends on the host we are running on
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostConfig {
  /// An integer value representing this container's relative CPU weight versus other containers.
  #[serde(rename = "CpuShares")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_shares: Option<i64>,

  /// Memory limit in bytes.
  #[serde(rename = "Memory")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory: Option<i64>,

  /// Path to `cgroups` under which the container's `cgroup` is created. If the path is not absolute, the path is considered to be relative to the `cgroups` path of the init process. Cgroups are created if they do not already exist.
  #[serde(rename = "CgroupParent")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cgroup_parent: Option<String>,

  /// Block IO weight (relative weight).
  #[serde(rename = "BlkioWeight")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_weight: Option<u16>,

  /// Block IO weight (relative device weight) in the form:  ``` [{\"Path\": \"device_path\", \"Weight\": weight}] ```
  #[serde(rename = "BlkioWeightDevice")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_weight_device: Option<Vec<ResourcesBlkioWeightDevice>>,

  /// Limit read rate (bytes per second) from a device, in the form:  ``` [{\"Path\": \"device_path\", \"Rate\": rate}] ```
  #[serde(rename = "BlkioDeviceReadBps")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_device_read_bps: Option<Vec<ThrottleDevice>>,

  /// Limit write rate (bytes per second) to a device, in the form:  ``` [{\"Path\": \"device_path\", \"Rate\": rate}] ```
  #[serde(rename = "BlkioDeviceWriteBps")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_device_write_bps: Option<Vec<ThrottleDevice>>,

  /// Limit read rate (IO per second) from a device, in the form:  ``` [{\"Path\": \"device_path\", \"Rate\": rate}] ```
  #[serde(rename = "BlkioDeviceReadIOps")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_device_read_iops: Option<Vec<ThrottleDevice>>,

  /// Limit write rate (IO per second) to a device, in the form:  ``` [{\"Path\": \"device_path\", \"Rate\": rate}] ```
  #[serde(rename = "BlkioDeviceWriteIOps")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blkio_device_write_iops: Option<Vec<ThrottleDevice>>,

  /// The length of a CPU period in microseconds.
  #[serde(rename = "CpuPeriod")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_period: Option<i64>,

  /// Microseconds of CPU time that the container can get in a CPU period.
  #[serde(rename = "CpuQuota")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_quota: Option<i64>,

  /// The length of a CPU real-time period in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
  #[serde(rename = "CpuRealtimePeriod")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_realtime_period: Option<i64>,

  /// The length of a CPU real-time runtime in microseconds. Set to 0 to allocate no time allocated to real-time tasks.
  #[serde(rename = "CpuRealtimeRuntime")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_realtime_runtime: Option<i64>,

  /// CPUs in which to allow execution (e.g., `0-3`, `0,1`).
  #[serde(rename = "CpusetCpus")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpuset_cpus: Option<String>,

  /// Memory nodes (MEMs) in which to allow execution (0-3, 0,1). Only effective on NUMA systems.
  #[serde(rename = "CpusetMems")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpuset_mems: Option<String>,

  /// A list of devices to add to the container.
  #[serde(rename = "Devices")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub devices: Option<Vec<DeviceMapping>>,

  /// a list of cgroup rules to apply to the container
  #[serde(rename = "DeviceCgroupRules")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub device_cgroup_rules: Option<Vec<String>>,

  /// A list of requests for devices to be sent to device drivers.
  #[serde(rename = "DeviceRequests")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub device_requests: Option<Vec<DeviceRequest>>,

  /// Kernel memory limit in bytes.  <p><br /></p>  > **Deprecated**: This field is deprecated as the kernel 5.4 deprecated > `kmem.limit_in_bytes`.
  #[serde(rename = "KernelMemory")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kernel_memory: Option<i64>,

  /// Hard limit for kernel TCP buffer memory (in bytes).
  #[serde(rename = "KernelMemoryTCP")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kernel_memory_tcp: Option<i64>,

  /// Memory soft limit in bytes.
  #[serde(rename = "MemoryReservation")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_reservation: Option<i64>,

  /// Total memory limit (memory + swap). Set as `-1` to enable unlimited swap.
  #[serde(rename = "MemorySwap")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_swap: Option<i64>,

  /// Tune a container's memory swappiness behavior. Accepts an integer between 0 and 100.
  #[serde(rename = "MemorySwappiness")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub memory_swappiness: Option<i64>,

  /// CPU quota in units of 10<sup>-9</sup> CPUs.
  #[serde(rename = "NanoCpus")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub nano_cpus: Option<i64>,

  /// Disable OOM Killer for the container.
  #[serde(rename = "OomKillDisable")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub oom_kill_disable: Option<bool>,

  /// Run an init inside the container that forwards signals and reaps processes. This field is omitted if empty, and the default (as configured on the daemon) is used.
  #[serde(rename = "Init")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub init: Option<bool>,

  /// Tune a container's PIDs limit. Set `0` or `-1` for unlimited, or `null` to not change.
  #[serde(rename = "PidsLimit")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub pids_limit: Option<i64>,

  /// A list of resource limits to set in the container. For example:  ``` {\"Name\": \"nofile\", \"Soft\": 1024, \"Hard\": 2048} ```
  #[serde(rename = "Ulimits")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ulimits: Option<Vec<ResourcesUlimits>>,

  /// The number of usable CPUs (Windows only).  On Windows Server containers, the processor resource controls are mutually exclusive. The order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
  #[serde(rename = "CpuCount")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_count: Option<i64>,

  /// The usable percentage of the available CPUs (Windows only).  On Windows Server containers, the processor resource controls are mutually exclusive. The order of precedence is `CPUCount` first, then `CPUShares`, and `CPUPercent` last.
  #[serde(rename = "CpuPercent")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cpu_percent: Option<i64>,

  /// Maximum IOps for the container system drive (Windows only)
  #[serde(rename = "IOMaximumIOps")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub io_maximum_iops: Option<i64>,

  /// Maximum IO in bytes per second for the container system drive (Windows only).
  #[serde(rename = "IOMaximumBandwidth")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub io_maximum_bandwidth: Option<i64>,

  /// A list of volume bindings for this container. Each volume binding is a string in one of these forms:  - `host-src:container-dest[:options]` to bind-mount a host path   into the container. Both `host-src`, and `container-dest` must   be an _absolute_ path. - `volume-name:container-dest[:options]` to bind-mount a volume   managed by a volume driver into the container. `container-dest`   must be an _absolute_ path.  `options` is an optional, comma-delimited list of:  - `nocopy` disables automatic copying of data from the container   path to the volume. The `nocopy` flag only applies to named volumes. - `[ro|rw]` mounts a volume read-only or read-write, respectively.   If omitted or set to `rw`, volumes are mounted read-write. - `[z|Z]` applies SELinux labels to allow or deny multiple containers   to read and write to the same volume.     - `z`: a _shared_ content label is applied to the content. This       label indicates that multiple containers can share the volume       content, for both reading and writing.     - `Z`: a _private unshared_ label is applied to the content.       This label indicates that only the current container can use       a private volume. Labeling systems such as SELinux require       proper labels to be placed on volume content that is mounted       into a container. Without a label, the security system can       prevent a container's processes from using the content. By       default, the labels set by the host operating system are not       modified. - `[[r]shared|[r]slave|[r]private]` specifies mount   [propagation behavior](https://www.kernel.org/doc/Documentation/filesystems/sharedsubtree.txt).   This only applies to bind-mounted volumes, not internal volumes   or named volumes. Mount propagation requires the source mount   point (the location where the source directory is mounted in the   host operating system) to have the correct propagation properties.   For shared volumes, the source mount point must be set to `shared`.   For slave volumes, the mount must be set to either `shared` or   `slave`.
  #[serde(rename = "Binds")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub binds: Option<Vec<String>>,

  /// Path to a file where the container ID is written
  #[serde(rename = "ContainerIDFile")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub container_id_file: Option<String>,

  #[serde(rename = "LogConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub log_config: Option<HostConfigLogConfig>,

  /// Network mode to use for this container. Supported standard values are: `bridge`, `host`, `none`, and `container:<name|id>`. Any other value is taken as a custom network's name to which this container should connect to.
  #[serde(rename = "NetworkMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub network_mode: Option<String>,

  #[serde(rename = "PortBindings")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub port_bindings: Option<PortMap>,

  #[serde(rename = "RestartPolicy")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub restart_policy: Option<RestartPolicy>,

  /// Automatically remove the container when the container's process exits. This has no effect if `RestartPolicy` is set.
  #[serde(rename = "AutoRemove")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub auto_remove: Option<bool>,

  /// Driver that this container uses to mount volumes.
  #[serde(rename = "VolumeDriver")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volume_driver: Option<String>,

  /// A list of volumes to inherit from another container, specified in the form `<container name>[:<ro|rw>]`.
  #[serde(rename = "VolumesFrom")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volumes_from: Option<Vec<String>>,

  /// Specification for mounts to be added to the container.
  #[serde(rename = "Mounts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mounts: Option<Vec<Mount>>,

  /// A list of kernel capabilities to add to the container. Conflicts with option 'Capabilities'.
  #[serde(rename = "CapAdd")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cap_add: Option<Vec<String>>,

  /// A list of kernel capabilities to drop from the container. Conflicts with option 'Capabilities'.
  #[serde(rename = "CapDrop")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cap_drop: Option<Vec<String>>,

  /// cgroup namespace mode for the container. Possible values are:  - `\"private\"`: the container runs in its own private cgroup namespace - `\"host\"`: use the host system's cgroup namespace  If not specified, the daemon default is used, which can either be `\"private\"` or `\"host\"`, depending on daemon version, kernel support and configuration.
  #[serde(rename = "CgroupnsMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cgroupns_mode: Option<HostConfigCgroupnsModeEnum>,

  /// A list of DNS servers for the container to use.
  #[serde(rename = "Dns")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dns: Option<Vec<String>>,

  /// A list of DNS options.
  #[serde(rename = "DnsOptions")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dns_options: Option<Vec<String>>,

  /// A list of DNS search domains.
  #[serde(rename = "DnsSearch")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dns_search: Option<Vec<String>>,

  /// A list of hostnames/IP mappings to add to the container's `/etc/hosts` file. Specified in the form `[\"hostname:IP\"]`.
  #[serde(rename = "ExtraHosts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub extra_hosts: Option<Vec<String>>,

  /// A list of additional groups that the container process will run as.
  #[serde(rename = "GroupAdd")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub group_add: Option<Vec<String>>,

  /// IPC sharing mode for the container. Possible values are:  - `\"none\"`: own private IPC namespace, with /dev/shm not mounted - `\"private\"`: own private IPC namespace - `\"shareable\"`: own private IPC namespace, with a possibility to share it with other containers - `\"container:<name|id>\"`: join another (shareable) container's IPC namespace - `\"host\"`: use the host system's IPC namespace  If not specified, daemon default is used, which can either be `\"private\"` or `\"shareable\"`, depending on daemon version and configuration.
  #[serde(rename = "IpcMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipc_mode: Option<String>,

  /// Cgroup to use for the container.
  #[serde(rename = "Cgroup")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cgroup: Option<String>,

  /// A list of links for the container in the form `container_name:alias`.
  #[serde(rename = "Links")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub links: Option<Vec<String>>,

  /// An integer value containing the score given to the container in order to tune OOM killer preferences.
  #[serde(rename = "OomScoreAdj")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub oom_score_adj: Option<i64>,

  /// Set the PID (Process) Namespace mode for the container. It can be either:  - `\"container:<name|id>\"`: joins another container's PID namespace - `\"host\"`: use the host's PID namespace inside the container
  #[serde(rename = "PidMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub pid_mode: Option<String>,

  /// Gives the container full access to the host.
  #[serde(rename = "Privileged")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub privileged: Option<bool>,

  /// Allocates an ephemeral host port for all of a container's exposed ports.  Ports are de-allocated when the container stops and allocated when the container starts. The allocated port might be changed when restarting the container.  The port is selected from the ephemeral port range that depends on the kernel. For example, on Linux the range is defined by `/proc/sys/net/ipv4/ip_local_port_range`.
  #[serde(rename = "PublishAllPorts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub publish_all_ports: Option<bool>,

  /// Mount the container's root filesystem as read only.
  #[serde(rename = "ReadonlyRootfs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub readonly_rootfs: Option<bool>,

  /// A list of string values to customize labels for MLS systems, such as SELinux.
  #[serde(rename = "SecurityOpt")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub security_opt: Option<Vec<String>>,

  /// Storage driver options for this container, in the form `{\"size\": \"120G\"}`.
  #[serde(rename = "StorageOpt")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub storage_opt: Option<HashMap<String, String>>,

  /// A map of container directories which should be replaced by tmpfs mounts, and their corresponding mount options. For example:  ``` { \"/run\": \"rw,noexec,nosuid,size=65536k\" } ```
  #[serde(rename = "Tmpfs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tmpfs: Option<HashMap<String, String>>,

  /// UTS namespace to use for the container.
  #[serde(rename = "UTSMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub uts_mode: Option<String>,

  /// Sets the usernamespace mode for the container when usernamespace remapping option is enabled.
  #[serde(rename = "UsernsMode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub userns_mode: Option<String>,

  /// Size of `/dev/shm` in bytes. If omitted, the system uses 64MB.
  #[serde(rename = "ShmSize")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub shm_size: Option<i64>,

  /// A list of kernel parameters (sysctls) to set in the container. For example:  ``` {\"net.ipv4.ip_forward\": \"1\"} ```
  #[serde(rename = "Sysctls")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sysctls: Option<HashMap<String, String>>,

  /// Runtime to use with this container.
  #[serde(rename = "Runtime")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub runtime: Option<String>,

  /// Initial console size, as an `[height, width]` array. (Windows only)
  #[serde(rename = "ConsoleSize")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub console_size: Option<Vec<i32>>,

  /// Isolation technology of the container. (Windows only)
  #[serde(rename = "Isolation")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub isolation: Option<HostConfigIsolationEnum>,

  /// The list of paths to be masked inside the container (this overrides the default set of paths).
  #[serde(rename = "MaskedPaths")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub masked_paths: Option<Vec<String>>,

  /// The list of paths to be set as read-only inside the container (this overrides the default set of paths).
  #[serde(rename = "ReadonlyPaths")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub readonly_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourcesBlkioWeightDevice {
  #[serde(rename = "Path")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,

  #[serde(rename = "Weight")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub weight: Option<usize>,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum HostConfigCgroupnsModeEnum {
  #[serde(rename = "")]
  EMPTY,
  #[serde(rename = "private")]
  PRIVATE,
  #[serde(rename = "host")]
  HOST,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mount {
  /// Container path.
  #[serde(rename = "Target")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub target: Option<String>,

  /// Mount source (e.g. a volume name, a host path).
  #[serde(rename = "Source")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,

  /// The mount type. Available types:  - `bind` Mounts a file or directory from the host into the container. Must exist prior to creating the container. - `volume` Creates a volume with the given name and options (or uses a pre-existing volume with the same name and options). These are **not** removed when the container is removed. - `tmpfs` Create a tmpfs with the given options. The mount source cannot be specified for tmpfs. - `npipe` Mounts a named pipe from the host into the container. Must exist prior to creating the container.
  #[serde(rename = "Type")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub typ: Option<MountTypeEnum>,

  /// Whether the mount should be read-only.
  #[serde(rename = "ReadOnly")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub read_only: Option<bool>,

  /// The consistency requirement for the mount: `default`, `consistent`, `cached`, or `delegated`.
  #[serde(rename = "Consistency")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub consistency: Option<String>,

  #[serde(rename = "BindOptions")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub bind_options: Option<MountBindOptions>,

  #[serde(rename = "VolumeOptions")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volume_options: Option<MountVolumeOptions>,

  #[serde(rename = "TmpfsOptions")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tmpfs_options: Option<MountTmpfsOptions>,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum MountTypeEnum {
  #[serde(rename = "")]
  EMPTY,
  #[serde(rename = "bind")]
  BIND,
  #[serde(rename = "volume")]
  VOLUME,
  #[serde(rename = "tmpfs")]
  TMPFS,
  #[serde(rename = "npipe")]
  NPIPE,
}

/// Optional configuration for the `bind` type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MountBindOptions {
  /// A propagation mode with the value `[r]private`, `[r]shared`, or `[r]slave`.
  #[serde(rename = "Propagation")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub propagation: Option<MountBindOptionsPropagationEnum>,

  /// Disable recursive bind mount.
  #[serde(rename = "NonRecursive")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub non_recursive: Option<bool>,
}

/// Optional configuration for the `volume` type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MountVolumeOptions {
  /// Populate volume with data from the target.
  #[serde(rename = "NoCopy")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub no_copy: Option<bool>,

  /// User-defined key/value metadata.
  #[serde(rename = "Labels")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub labels: Option<HashMap<String, String>>,

  #[serde(rename = "DriverConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub driver_config: Option<MountVolumeOptionsDriverConfig>,
}

/// Optional configuration for the `tmpfs` type.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MountTmpfsOptions {
  /// The size for the tmpfs mount in bytes.
  #[serde(rename = "SizeBytes")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size_bytes: Option<i64>,

  /// The permission mode for the tmpfs mount in an integer.
  #[serde(rename = "Mode")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mode: Option<i64>,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum MountBindOptionsPropagationEnum {
  #[serde(rename = "")]
  EMPTY,
  #[serde(rename = "private")]
  PRIVATE,
  #[serde(rename = "rprivate")]
  RPRIVATE,
  #[serde(rename = "shared")]
  SHARED,
  #[serde(rename = "rshared")]
  RSHARED,
  #[serde(rename = "slave")]
  SLAVE,
  #[serde(rename = "rslave")]
  RSLAVE,
}

/// Map of driver specific options
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MountVolumeOptionsDriverConfig {
  /// Name of the driver to use to create the volume.
  #[serde(rename = "Name")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,

  /// key/value map of driver specific options.
  #[serde(rename = "Options")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub options: Option<HashMap<String, String>>,
}

/// This container's networking configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(missing_docs)]
pub struct NetworkingConfig<T: Into<String> + Hash + Eq> {
  pub endpoints_config: HashMap<T, EndpointSettings>,
}

/// Configuration for a network endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EndpointSettings {
  #[serde(rename = "IPAMConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipam_config: Option<EndpointIpamConfig>,

  #[serde(rename = "Links")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub links: Option<Vec<String>>,

  #[serde(rename = "Aliases")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub aliases: Option<Vec<String>>,

  /// Unique ID of the network.
  #[serde(rename = "NetworkID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub network_id: Option<String>,

  /// Unique ID for the service endpoint in a Sandbox.
  #[serde(rename = "EndpointID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub endpoint_id: Option<String>,

  /// Gateway address for this network.
  #[serde(rename = "Gateway")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub gateway: Option<String>,

  /// IPv4 address.
  #[serde(rename = "IPAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ip_address: Option<String>,

  /// Mask length of the IPv4 address.
  #[serde(rename = "IPPrefixLen")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ip_prefix_len: Option<i64>,

  /// IPv6 gateway address.
  #[serde(rename = "IPv6Gateway")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv6_gateway: Option<String>,

  /// Global IPv6 address.
  #[serde(rename = "GlobalIPv6Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub global_ipv6_address: Option<String>,

  /// Mask length of the global IPv6 address.
  #[serde(rename = "GlobalIPv6PrefixLen")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub global_ipv6_prefix_len: Option<i64>,

  /// MAC address for the endpoint on this network.
  #[serde(rename = "MacAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mac_address: Option<String>,

  /// DriverOpts is a mapping of driver options and values. These options are passed directly to the driver and are driver specific.
  #[serde(rename = "DriverOpts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub driver_opts: Option<HashMap<String, String>>,
}

/// EndpointIPAMConfig represents an endpoint's IPAM configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EndpointIpamConfig {
  #[serde(rename = "IPv4Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv4_address: Option<String>,

  #[serde(rename = "IPv6Address")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ipv6_address: Option<String>,

  #[serde(rename = "LinkLocalIPs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub link_local_i_ps: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThrottleDevice {
  /// Device path
  #[serde(rename = "Path")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub path: Option<String>,

  /// Rate
  #[serde(rename = "Rate")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub rate: Option<i64>,
}

/// A device mapping between the host and container
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceMapping {
  #[serde(rename = "PathOnHost")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub path_on_host: Option<String>,

  #[serde(rename = "PathInContainer")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub path_in_container: Option<String>,

  #[serde(rename = "CgroupPermissions")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cgroup_permissions: Option<String>,
}

/// A request for devices to be sent to device drivers
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceRequest {
  #[serde(rename = "Driver")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub driver: Option<String>,

  #[serde(rename = "Count")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub count: Option<i64>,

  #[serde(rename = "DeviceIDs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub device_ids: Option<Vec<String>>,

  /// A list of capabilities; an OR list of AND lists of capabilities.
  #[serde(rename = "Capabilities")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub capabilities: Option<Vec<Vec<String>>>,

  /// Driver-specific options, specified as a key/value pairs. These options are passed directly to the driver.
  #[serde(rename = "Options")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub options: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourcesUlimits {
  /// Name of ulimit
  #[serde(rename = "Name")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,

  /// Soft limit
  #[serde(rename = "Soft")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub soft: Option<i64>,

  /// Hard limit
  #[serde(rename = "Hard")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hard: Option<i64>,
}

/// The logging configuration for this container
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostConfigLogConfig {
  #[serde(rename = "Type")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub typ: Option<String>,

  #[serde(rename = "Config")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub config: Option<HashMap<String, String>>,
}

/// PortBinding represents a binding between a host IP address and a host port.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PortBinding {
  /// Host IP address that the container's port is mapped to.
  #[serde(rename = "HostIp")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub host_ip: Option<String>,

  /// Host port number that the container's port is mapped to.
  #[serde(rename = "HostPort")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub host_port: Option<String>,
}

/// The behavior to apply when the container exits. The default is not to restart.  An ever increasing delay (double the previous delay, starting at 100ms) is added before each restart to prevent flooding the server.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RestartPolicy {
  /// - Empty string means not to restart - `no` Do not automatically restart - `always` Always restart - `unless-stopped` Restart always except when the user has manually stopped the container - `on-failure` Restart only when the container exit code is non-zero
  #[serde(rename = "Name")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<RestartPolicyNameEnum>,

  /// If `on-failure` is used, the number of times to retry before giving up.
  #[serde(rename = "MaximumRetryCount")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub maximum_retry_count: Option<i64>,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum RestartPolicyNameEnum {
  #[serde(rename = "")]
  EMPTY,
  #[serde(rename = "no")]
  NO,
  #[serde(rename = "always")]
  ALWAYS,
  #[serde(rename = "unless-stopped")]
  UNLESS_STOPPED,
  #[serde(rename = "on-failure")]
  ON_FAILURE,
}

#[allow(non_camel_case_types)]
#[derive(
  Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Eq, Ord,
)]
pub enum HostConfigIsolationEnum {
  #[serde(rename = "")]
  EMPTY,
  #[serde(rename = "default")]
  DEFAULT,
  #[serde(rename = "process")]
  PROCESS,
  #[serde(rename = "hyperv")]
  HYPERV,
}

impl ::std::fmt::Display for HostConfigIsolationEnum {
  fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
    match *self {
      HostConfigIsolationEnum::EMPTY => write!(f, ""),
      HostConfigIsolationEnum::DEFAULT => write!(f, "{}", "default"),
      HostConfigIsolationEnum::PROCESS => write!(f, "{}", "process"),
      HostConfigIsolationEnum::HYPERV => write!(f, "{}", "hyperv"),
    }
  }
}

impl ::std::str::FromStr for HostConfigIsolationEnum {
  type Err = String;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "" => Ok(HostConfigIsolationEnum::EMPTY),
      "default" => Ok(HostConfigIsolationEnum::DEFAULT),
      "process" => Ok(HostConfigIsolationEnum::PROCESS),
      "hyperv" => Ok(HostConfigIsolationEnum::HYPERV),
      x => Err(format!("Invalid enum type: {}", x)),
    }
  }
}

impl ::std::convert::AsRef<str> for HostConfigIsolationEnum {
  fn as_ref(&self) -> &str {
    match self {
      HostConfigIsolationEnum::EMPTY => "",
      HostConfigIsolationEnum::DEFAULT => "default",
      HostConfigIsolationEnum::PROCESS => "process",
      HostConfigIsolationEnum::HYPERV => "hyperv",
    }
  }
}
