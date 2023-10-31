use std::{io, fs};

use utoipa::openapi::{RefOr, Schema};
use utoipa::{openapi, OpenApi};
use clap::*;
use bollard_next::container::{Config};
use bollard_next::models::{
  HealthConfig, HostConfig, DeviceMapping, ThrottleDevice, DeviceRequest,
  ResourcesBlkioWeightDevice, ResourcesUlimits, HostConfigLogConfig,
  RestartPolicy, RestartPolicyNameEnum, Mount, MountTypeEnum, MountBindOptions,
  MountBindOptionsPropagationEnum, MountVolumeOptions,
  MountVolumeOptionsDriverConfig, MountTmpfsOptions,
  HostConfigCgroupnsModeEnum, HostConfigIsolationEnum, NetworkingConfig,
  EndpointSettings, EndpointIpamConfig,
};

use nanocl_stubs::cert_manager::CertManagerIssuer;
use nanocl_stubs::cargo_config::{
  CargoConfigPartial, ReplicationMode, ReplicationStatic,
};

include!("./src/cli.rs");
include!("./build_schema.rs");

#[derive(OpenApi)]
#[openapi(components(schemas(
  CertManagerIssuer,
  EmptyObject,
  CargoConfigPartial,
  Config,
  RestartPolicyNameEnum,
  EndpointSettings,
  EndpointIpamConfig,
  HealthConfig,
  RestartPolicy,
  HostConfig,
  ThrottleDevice,
  DeviceMapping,
  ResourcesBlkioWeightDevice,
  DeviceRequest,
  ResourcesUlimits,
  HostConfigLogConfig,
  PortMap,
  Mount,
  MountTypeEnum,
  MountBindOptions,
  MountBindOptionsPropagationEnum,
  MountVolumeOptions,
  MountVolumeOptionsDriverConfig,
  MountTmpfsOptions,
  HostConfigCgroupnsModeEnum,
  HostConfigIsolationEnum,
  NetworkingConfig,
  ReplicationMode,
  ReplicationStatic
)))]
struct ApiDoc;

const MAN_PATH: &str = "./target/man";
const RESOURCE_SCHEMA_FILENAME: &str = "cargo_config.json";

/// Set the git commit hash as an environment variable
fn set_env_git_commit_hash() -> io::Result<()> {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()?;

  let git_hash = String::from_utf8(output.stdout).unwrap();

  println!("cargo:rustc-env=GIT_HASH={git_hash}");

  Ok(())
}

/// Set the target architecture as an environment variable
fn set_env_target_arch() -> io::Result<()> {
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH")
    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

  println!("cargo:rustc-env=TARGET_ARCH={arch}");

  Ok(())
}

fn set_channel() -> io::Result<()> {
  let channel = std::env::var("NANOCL_CHANNEL").unwrap_or("stable".into());
  println!("cargo:rustc-env=CHANNEL={channel}");
  Ok(())
}

/// Generate ncertmanager man page
pub fn generate_man_command(
  name: &str,
  app: clap::Command,
) -> std::io::Result<()> {
  let man = clap_mangen::Man::new(app);
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(out_dir.join(format!("{MAN_PATH}/{name}.1")), man_buffer)?;

  Ok(())
}

fn main() -> std::io::Result<()> {
  set_channel()?;
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  std::fs::create_dir_all(MAN_PATH)?;
  generate_man_command("ncertmanager", Cli::command())?;
  generate_cargo_config_schema();

  Ok(())
}
