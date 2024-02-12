use bollard_next::container::RemoveContainerOptions;

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::{
  vm::{Vm, VmInspect},
  process::ProcessKind,
  vm_spec::VmSpecPartial,
  system::NativeEventAction,
};

use crate::{
  utils,
  repositories::generic::*,
  models::{
    VmDb, SystemState, VmObjCreateIn, VmImageDb, SpecDb, VmObjPutIn,
    VmObjPatchIn, ProcessDb,
  },
};
use super::generic::*;

impl ObjProcess for VmDb {
  fn get_process_kind() -> ProcessKind {
    ProcessKind::Vm
  }
}

impl ObjCreate for VmDb {
  type ObjCreateIn = VmObjCreateIn;
  type ObjCreateOut = Vm;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let name = &obj.spec.name;
    let namespace = &obj.namespace;
    let version = &obj.version;
    log::debug!(
      "Creating VM {name} in namespace {namespace} with version: {version}",
    );
    let vm_key = utils::key::gen_key(namespace, name);
    let mut vm = obj.spec.clone();
    if VmDb::read_by_pk(&vm_key, &state.pool).await.is_ok() {
      return Err(HttpError::conflict(format!(
        "VM with name {name} already exists in namespace {namespace}",
      )));
    }
    let image = VmImageDb::read_by_pk(&vm.disk.image, &state.pool).await?;
    if image.kind.as_str() != "Base" {
      return Err(HttpError::bad_request(format!("Image {} is not a base image please convert the snapshot into a base image first", &vm.disk.image)));
    }
    let snap_name = format!("{}.{vm_key}", &image.name);
    let size = vm.disk.size.unwrap_or(20);
    log::debug!("Creating snapshot {snap_name} with size {size}");
    let image =
      utils::vm_image::create_snap(&snap_name, size, &image, state).await?;
    log::debug!("Snapshot {snap_name} created");
    // Use the snapshot image
    vm.disk.image = image.name.clone();
    vm.disk.size = Some(size);
    let vm =
      VmDb::create_from_spec(namespace, &vm, version, &state.pool).await?;
    utils::vm::create_instance(&vm, &image, true, state).await?;
    Ok(vm)
  }
}

impl ObjDelByPk for VmDb {
  type ObjDelOpts = ();
  type ObjDelOut = Vm;

  fn get_del_event() -> NativeEventAction {
    NativeEventAction::Destroying
  }

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let vm = VmDb::transform_read_by_pk(pk, &state.pool).await?;
    let options = bollard_next::container::RemoveContainerOptions {
      force: true,
      ..Default::default()
    };
    let container_name = format!("{}.v", pk);
    utils::container::delete_instance(&container_name, Some(options), state)
      .await?;
    VmDb::del_by_pk(pk, &state.pool).await?;
    SpecDb::del_by_kind_key(pk, &state.pool).await?;
    utils::vm_image::delete_by_pk(&vm.spec.disk.image, state).await?;
    Ok(vm)
  }
}

impl ObjPutByPk for VmDb {
  type ObjPutIn = VmObjPutIn;
  type ObjPutOut = Vm;

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    let vm = VmDb::transform_read_by_pk(pk, &state.pool).await?;
    let container_name = format!("{}.v", &vm.spec.vm_key);
    utils::container::stop_instances(pk, &ProcessKind::Vm, state).await?;
    utils::container::delete_instance(
      &container_name,
      None::<RemoveContainerOptions>,
      state,
    )
    .await?;
    let vm = VmDb::update_from_spec(
      &vm.spec.vm_key,
      &obj.spec,
      &obj.version,
      &state.pool,
    )
    .await?;
    let image = VmImageDb::read_by_pk(&vm.spec.disk.image, &state.pool).await?;
    utils::vm::create_instance(&vm, &image, false, state).await?;
    // VmDb::start_process_by_kind_key(&vm.spec.vm_key, state).await?;
    Ok(vm)
  }
}

impl ObjPatchByPk for VmDb {
  type ObjPatchIn = VmObjPatchIn;
  type ObjPatchOut = Vm;

  async fn fn_patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut> {
    let spec = &obj.spec;
    let version = &obj.version;
    let vm = VmDb::transform_read_by_pk(pk, &state.pool).await?;
    let old_spec = SpecDb::read_by_pk(&vm.spec.key, &state.pool)
      .await?
      .try_to_vm_spec()?;
    let vm_partial = VmSpecPartial {
      name: spec.name.to_owned().unwrap_or(vm.spec.name.clone()),
      disk: old_spec.disk,
      host_config: Some(
        spec.host_config.to_owned().unwrap_or(old_spec.host_config),
      ),
      hostname: if spec.hostname.is_some() {
        spec.hostname.clone()
      } else {
        old_spec.hostname
      },
      user: if spec.user.is_some() {
        spec.user.clone()
      } else {
        old_spec.user
      },
      password: if spec.password.is_some() {
        spec.password.clone()
      } else {
        old_spec.password
      },
      ssh_key: if spec.ssh_key.is_some() {
        spec.ssh_key.clone()
      } else {
        old_spec.ssh_key
      },
      mac_address: old_spec.mac_address,
      labels: if spec.labels.is_some() {
        spec.labels.clone()
      } else {
        old_spec.labels
      },
      metadata: if spec.metadata.is_some() {
        spec.metadata.clone()
      } else {
        old_spec.metadata
      },
    };
    let obj = &VmObjPutIn {
      spec: vm_partial,
      version: version.to_owned(),
    };
    VmDb::fn_put_obj_by_pk(pk, obj, state).await
  }
}

impl ObjInspectByPk for VmDb {
  type ObjInspectOut = VmInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let vm = VmDb::transform_read_by_pk(pk, &state.pool).await?;
    let processes =
      ProcessDb::read_by_kind_key(&vm.spec.vm_key, &state.pool).await?;
    let (_, _, _, running_instances) =
      utils::container::count_status(&processes);
    Ok(VmInspect {
      created_at: vm.created_at,
      namespace_name: vm.namespace_name,
      spec: vm.spec,
      instance_total: processes.len(),
      instance_running: running_instances,
      instances: processes,
    })
  }
}
