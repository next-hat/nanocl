use std::collections::HashMap;

use ntex::http::StatusCode;
use bollard_next::Docker;
use bollard_next::service::{HostConfig, DeviceMapping};
use bollard_next::container::{CreateContainerOptions, StartContainerOptions};

use nanocl_stubs::vm::Vm;
use nanocl_stubs::config::DaemonConfig;
use nanocl_stubs::vm_config::VmConfigPartial;

use crate::{utils, repositories};
use crate::error::HttpResponseError;
use crate::models::Pool;

pub async fn create(
  mut vm: VmConfigPartial,
  namespace: &str,
  version: String,
  daemon_conf: &DaemonConfig,
  docker_api: &Docker,
  pool: &Pool,
) -> Result<Vm, HttpResponseError> {
  if repositories::vm::find_by_key(vm.name.clone(), pool)
    .await
    .is_ok()
  {
    return Err(HttpResponseError {
      status: StatusCode::BAD_REQUEST,
      msg: format!("VM with name {} already exists", vm.name),
    });
  }

  let vmimagespath = format!("{}/vms/images", daemon_conf.state_dir);

  let image = repositories::vm_image::find_by_name(&vm.image, pool).await?;

  let image =
    utils::vm_image::create_snap(&vm.name, &image, daemon_conf, pool).await?;

  vm.image = image.name;

  let mut labels = HashMap::new();
  labels.insert("io.nanocl.vm", vm.name.as_str());
  labels.insert("io.nanocl.vmnsp", namespace);

  let config = bollard_next::container::Config {
    image: Some("nanocl-qemu:dev"),
    tty: Some(true),
    labels: Some(labels),
    cmd: Some(vec![
      "-accel",
      "kvm",
      "-m",
      "4G",
      "-smp 4",
      "-hda",
      &image.path,
      "--nographic",
    ]),
    host_config: Some(HostConfig {
      binds: Some(vec![format!("{vmimagespath}:/var/lib/nanocl/vms/images")]),
      devices: Some(vec![
        DeviceMapping {
          path_on_host: Some("/dev/kvm".into()),
          path_in_container: Some("/dev/kvm".into()),
          cgroup_permissions: Some("rwm".into()),
        },
        DeviceMapping {
          path_on_host: Some("/dev/net/tun".into()),
          path_in_container: Some("/dev/net/tun".into()),
          cgroup_permissions: Some("rwm".into()),
        },
      ]),
      cap_add: Some(vec!["NET_ADMIN".into()]),
      ..Default::default()
    }),
    ..Default::default()
  };

  let options = Some(CreateContainerOptions {
    name: vm.name.clone(),
    ..Default::default()
  });

  let cnt = docker_api.create_container(options, config).await?;

  docker_api
    .start_container(&cnt.id, None::<StartContainerOptions<String>>)
    .await?;

  let vm =
    repositories::vm::create(namespace.to_owned(), vm, version, pool).await?;

  Ok(vm)
}
