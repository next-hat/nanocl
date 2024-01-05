use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::vm::Vm;

use crate::{
  utils,
  repositories::generic::*,
  models::{VmDb, SystemState, VmObjCreateIn, VmImageDb, SpecDb},
};
use super::generic::*;

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
    let snapname = format!("{}.{vm_key}", &image.name);
    let size = vm.disk.size.unwrap_or(20);
    log::debug!("Creating snapshot {snapname} with size {size}");
    let image =
      utils::vm_image::create_snap(&snapname, size, &image, state).await?;
    log::debug!("Snapshot {snapname} created");
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

  async fn fn_del_obj_by_pk(
    key: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let vm = VmDb::transform_read_by_pk(key, &state.pool).await?;
    let options = bollard_next::container::RemoveContainerOptions {
      force: true,
      ..Default::default()
    };
    let container_name = format!("{}.v", key);
    utils::process::remove(&container_name, Some(options), state).await?;
    VmDb::del_by_pk(key, &state.pool).await?;
    SpecDb::del_by_kind_key(key, &state.pool).await?;
    utils::vm_image::delete_by_name(&vm.spec.disk.image, &state.pool).await?;
    Ok(vm)
  }
}
