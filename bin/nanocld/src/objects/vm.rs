use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  resource_key::ResourceKey,
  system::{
    NativeEventAction, ObjPsHealthStatusKind, ObjPsStatus, ObjPsStatusKind,
    ObjPsStatusPartial,
  },
  vm::{Vm, VmInspect},
  vm_spec::VmSpecPartial,
};

use crate::{
  models::{
    ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb, SpecDb, SystemState, VmDb,
    VmObjCreateIn, VmObjPatchIn, VmObjPutIn,
  },
  repositories::generic::*,
  utils,
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
    let vm = &obj.spec;
    log::debug!(
      "Creating VM {name} in namespace {namespace} with version: {version}",
    );
    let vm_key = ResourceKey::new(name, namespace)
      .map_err(HttpError::bad_request)?
      .to_string();
    if VmDb::read_by_pk(&vm_key, &state.inner.pool).await.is_ok() {
      return Err(HttpError::conflict(format!(
        "VM with name {name} already exists in namespace {namespace}",
      )));
    }
    if name.contains('.') {
      return Err(HttpError::bad_request("VM name cannot contain '.'"));
    }
    let status = ObjPsStatusPartial {
      key: vm_key.clone(),
      wanted: ObjPsStatusKind::Create,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Create,
      prev_actual: ObjPsStatusKind::Create,
      health: ObjPsHealthStatusKind::Unknown,
    };
    let status: ObjPsStatus =
      ObjPsStatusDb::create_from(status, &state.inner.pool)
        .await?
        .try_into()?;
    let new_spec = SpecDb::try_from_vm_partial(&vm_key, version, vm)?;
    let spec = SpecDb::create_from(new_spec, &state.inner.pool)
      .await?
      .try_to_vm_spec()?;
    let new_item = VmDb {
      key: vm_key.to_owned(),
      name: vm.name.clone(),
      created_at: chrono::Utc::now().naive_utc(),
      namespace_name: namespace.clone(),
      spec_key: spec.key,
      status_key: vm_key,
    };
    let item = VmDb::create_from(new_item, &state.inner.pool).await?;
    let vm = item.with_spec(&(spec, status));
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
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Destroy.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Destroying.to_string()),
      prev_actual: Some(status.actual),
      ..Default::default()
    };
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
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
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Updating.to_string()),
      prev_actual: Some(status.actual),
      health: Some(ObjPsHealthStatusKind::Unknown.to_string()),
    };
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
    let vm = VmDb::update_from_spec(
      &vm.spec.vm_key,
      &obj.spec,
      &obj.version,
      &state.inner.pool,
    )
    .await?;
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
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let old_spec = SpecDb::read_by_pk(&vm.spec.key, &state.inner.pool)
      .await?
      .try_to_vm_spec()?;
    let vm_partial = VmSpecPartial {
      name: spec.name.to_owned().unwrap_or(vm.spec.name.clone()),
      image: old_spec.image,
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
      init_container: if spec.init_container.is_some() {
        spec.init_container.clone()
      } else {
        old_spec.init_container.clone()
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
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let processes =
      ProcessDb::read_by_kind_key(&vm.spec.vm_key, None, &state.inner.pool)
        .await?;
    let (total, _, _, running_instances) =
      utils::container::generic::count_status(&processes);
    Ok(VmInspect {
      created_at: vm.created_at,
      namespace_name: vm.namespace_name,
      spec: vm.spec,
      instance_total: total,
      instance_running: running_instances,
      instances: processes,
      status: vm.status,
    })
  }
}
