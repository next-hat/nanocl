use std::{collections::HashSet, sync::OnceLock};

use futures::lock::Mutex;
use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::{
  generic::{GenericClause, GenericFilter},
  resource_key::ResourceKey,
  system::{
    NativeEventAction, ObjPsHealthStatusKind, ObjPsStatus, ObjPsStatusKind,
    ObjPsStatusPartial,
  },
  vm::{Vm, VmInspect},
  vm_spec::{VmCloudInitUserData, VmSpecPartial},
};

use crate::{
  models::{
    ObjPsStatusDb, ProcessDb, SpecDb, SystemState, VmDb, VmObjCreateIn,
    VmObjPatchIn, VmObjPutIn, VmOperationDb, VmOperationOwner,
  },
  repositories::generic::*,
  utils,
};

use super::generic::*;

// MAC allocation and persistence must be serialized across VM names. The
// daemon owns VM allocation on this single node; per-VM lifecycle locks cannot
// prevent two concurrent creates from claiming the same explicit address.
fn mac_allocation_lock() -> &'static Mutex<()> {
  static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| Mutex::new(()))
}

fn merge_cloud_init_user_data(
  requested: Option<VmCloudInitUserData>,
  current: Option<VmCloudInitUserData>,
) -> Option<VmCloudInitUserData> {
  match requested {
    Some(VmCloudInitUserData::Reset) => None,
    Some(data) => Some(data),
    None => current,
  }
}

fn normalize_mac_address(value: &str) -> HttpResult<String> {
  let parts = value.split(':').collect::<Vec<_>>();
  if parts.len() != 6
    || parts.iter().any(|part| {
      part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
  {
    return Err(HttpError::bad_request(
      "MacAddress must contain six hexadecimal octets separated by colons",
    ));
  }
  let first = u8::from_str_radix(parts[0], 16).unwrap();
  if first & 1 != 0 || parts.iter().all(|part| *part == "00") {
    return Err(HttpError::bad_request(
      "MacAddress must be a nonzero unicast address",
    ));
  }
  Ok(value.to_ascii_lowercase())
}

fn select_mac_address(
  requested: Option<&str>,
  current: Option<&str>,
  in_use: &HashSet<String>,
) -> HttpResult<String> {
  let requested = requested.map(normalize_mac_address).transpose()?;
  let current = current.map(normalize_mac_address).transpose()?;
  if let (Some(requested), Some(current)) = (&requested, &current)
    && requested != current
  {
    return Err(HttpError::bad_request(
      "MacAddress is immutable; create a new VM to use another address",
    ));
  }
  if let Some(address) = current.or(requested) {
    if in_use.contains(&address) {
      return Err(HttpError::conflict(
        "MacAddress is already assigned to another VM",
      ));
    }
    return Ok(address);
  }
  loop {
    let id = uuid::Uuid::new_v4();
    let bytes = id.as_bytes();
    let address = format!(
      "02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
      bytes[0], bytes[1], bytes[2], bytes[3], bytes[4],
    );
    if !in_use.contains(&address) {
      return Ok(address);
    }
  }
}

async fn assigned_mac_addresses(
  except_key: &str,
  state: &SystemState,
) -> HttpResult<HashSet<String>> {
  // Only active specifications reserve addresses. Bounded key-ordered pages
  // avoid CockroachDB's pathological plan for a joined query with i64::MAX
  // LIMIT. A key cursor also avoids skipping rows if a VM is deleted while
  // scanning; creates and MAC changes already hold mac_allocation_lock.
  const PAGE_SIZE: usize = 256;
  let mut addresses = HashSet::new();
  let mut after = None;
  loop {
    let mut filter = GenericFilter::new().limit(PAGE_SIZE);
    filter.order_by = Some(vec!["key asc".into()]);
    if let Some(key) = after {
      filter = filter.r#where("key", GenericClause::Gt(key));
    }
    let page = VmDb::transform_read_by(&filter, &state.inner.pool).await?;
    let complete = page.len() < PAGE_SIZE;
    after = page.last().map(|vm| vm.spec.vm_key.clone());
    addresses.extend(
      page
        .into_iter()
        .filter(|vm| vm.spec.vm_key != except_key)
        .filter_map(|vm| vm.spec.mac_address)
        .map(|address| address.to_ascii_lowercase()),
    );
    if complete {
      return Ok(addresses);
    }
  }
}

impl VmDb {
  /// Include legacy MAC allocation in the accepted revision, before execution
  /// can mount a generation. A rejected start must not commit that backfill.
  pub(crate) async fn prepare_start(
    key: &str,
    owner: VmOperationOwner,
    state: &SystemState,
  ) -> HttpResult<Vm> {
    let _guard = mac_allocation_lock().lock().await;
    let vm = Self::transform_read_by_pk(key, &state.inner.pool).await?;
    let replacement = if let Some(address) = &vm.spec.mac_address {
      normalize_mac_address(address)?;
      None
    } else {
      let used = assigned_mac_addresses(key, state).await?;
      let mut spec = VmSpecPartial::from(vm.spec.clone());
      spec.mac_address = Some(select_mac_address(None, None, &used)?);
      Some(SpecDb::try_from_vm_partial(key, &vm.spec.version, &spec)?)
    };
    let revision = replacement.as_ref().map_or(vm.spec.key, |spec| spec.key);
    let operation =
      VmOperationDb::new(&owner, key, revision, ObjPsStatusKind::Starting);
    Ok(
      Self::accept_operation(&vm, replacement, operation, &state.inner.pool)
        .await?,
    )
  }

  // Both PUT and PATCH hold mac_allocation_lock before calling this helper.
  // PATCH must keep that guard from its initial read through persistence so
  // omitted values cannot restore a concurrently changed Console setting.
  async fn put_spec_unlocked(
    pk: &str,
    obj: &VmObjPutIn,
    state: &SystemState,
    owner: VmOperationOwner,
  ) -> HttpResult<Vm> {
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let mut spec = obj.spec.clone();
    spec.cloud_init_user_data =
      merge_cloud_init_user_data(spec.cloud_init_user_data, None);
    utils::container::vm::validate_spec(&spec)
      .map_err(|err| HttpError::bad_request(err.to_string()))?;
    let in_use = assigned_mac_addresses(pk, state).await?;
    spec.mac_address = Some(select_mac_address(
      spec.mac_address.as_deref(),
      vm.spec.mac_address.as_deref(),
      &in_use,
    )?);
    utils::container::vm::validate_replacement_spec(&vm, &spec, state).await?;
    let replacement = SpecDb::try_from_vm_partial(pk, &obj.version, &spec)?;
    let operation = VmOperationDb::new(
      &owner,
      pk,
      replacement.key,
      ObjPsStatusKind::Updating,
    );
    Ok(
      VmDb::accept_operation(
        &vm,
        Some(replacement),
        operation,
        &state.inner.pool,
      )
      .await?,
    )
  }

  /// Backfill a legacy VM before creating its next runtime container. This
  /// persists an ordinary spec revision without requesting a lifecycle task.
  pub async fn ensure_mac_address(
    key: &str,
    state: &SystemState,
  ) -> HttpResult<Vm> {
    let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
    if let Some(address) = &vm.spec.mac_address {
      normalize_mac_address(address)?;
      // Allocation checked uniqueness before persisting this immutable identity.
      return Ok(vm);
    }
    let _guard = mac_allocation_lock().lock().await;
    let vm = VmDb::transform_read_by_pk(key, &state.inner.pool).await?;
    if let Some(address) = &vm.spec.mac_address {
      normalize_mac_address(address)?;
      return Ok(vm);
    }
    let in_use = assigned_mac_addresses(key, state).await?;
    let address =
      select_mac_address(None, vm.spec.mac_address.as_deref(), &in_use)?;
    if vm.spec.mac_address.as_deref() == Some(address.as_str()) {
      return Ok(vm);
    }
    let version = vm.spec.version.clone();
    let mut spec = VmSpecPartial::from(vm.spec);
    spec.mac_address = Some(address);
    Ok(VmDb::update_from_spec(key, &spec, &version, &state.inner.pool).await?)
  }
}

impl ObjCreate for VmDb {
  type ObjCreateIn = VmObjCreateIn;
  type ObjCreateOut = Vm;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let _guard = mac_allocation_lock().lock().await;
    let name = &obj.spec.name;
    let namespace = &obj.namespace;
    let version = &obj.version;
    let mut vm = obj.spec.clone();
    vm.cloud_init_user_data =
      merge_cloud_init_user_data(vm.cloud_init_user_data, None);
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
    utils::container::vm::validate_spec(&vm)
      .map_err(|err| HttpError::bad_request(err.to_string()))?;
    let in_use = assigned_mac_addresses(&vm_key, state).await?;
    vm.mac_address = Some(select_mac_address(
      vm.mac_address.as_deref(),
      None,
      &in_use,
    )?);
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
    let new_spec = SpecDb::try_from_vm_partial(&vm_key, version, &vm)?;
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

  async fn del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let key = pk.to_owned();
    let prepare_state = state.clone();
    crate::tasks::vm::accept_task(
      pk,
      NativeEventAction::Destroying,
      state,
      move |owner| async move {
        let vm =
          Self::transform_read_by_pk(&key, &prepare_state.inner.pool).await?;
        let operation = VmOperationDb::new(
          &owner,
          &key,
          vm.spec.key,
          ObjPsStatusKind::Destroying,
        );
        Ok(
          Self::accept_operation(
            &vm,
            None,
            operation,
            &prepare_state.inner.pool,
          )
          .await?,
        )
      },
    )
    .await
  }

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    Self::del_obj_by_pk(pk, &(), state).await
  }
}

impl ObjPutByPk for VmDb {
  type ObjPutIn = VmObjPutIn;
  type ObjPutOut = Vm;

  async fn put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    let key = pk.to_owned();
    let prepare_state = state.clone();
    let obj = VmObjPutIn {
      spec: obj.spec.clone(),
      version: obj.version.clone(),
    };
    crate::tasks::vm::accept_task(
      pk,
      NativeEventAction::Updating,
      state,
      move |owner| async move {
        let _guard = mac_allocation_lock().lock().await;
        Self::put_spec_unlocked(&key, &obj, &prepare_state, owner).await
      },
    )
    .await
  }

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    Self::put_obj_by_pk(pk, obj, state).await
  }
}

impl ObjPatchByPk for VmDb {
  type ObjPatchIn = VmObjPatchIn;
  type ObjPatchOut = Vm;

  async fn patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut> {
    let key = pk.to_owned();
    let prepare_state = state.clone();
    let obj = VmObjPatchIn {
      spec: obj.spec.clone(),
      version: obj.version.clone(),
    };
    crate::tasks::vm::accept_task(
      pk,
      NativeEventAction::Updating,
      state,
      move |owner| async move {
        Self::prepare_patch(&key, &obj, &prepare_state, owner).await
      },
    )
    .await
  }

  async fn fn_patch_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPatchIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPatchOut> {
    Self::patch_obj_by_pk(pk, obj, state).await
  }
}

impl VmDb {
  async fn prepare_patch(
    pk: &str,
    obj: &VmObjPatchIn,
    state: &SystemState,
    owner: VmOperationOwner,
  ) -> HttpResult<Vm> {
    let _guard = mac_allocation_lock().lock().await;
    let spec = &obj.spec;
    let version = &obj.version;
    let vm = VmDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let old_spec = SpecDb::read_by_pk(&vm.spec.key, &state.inner.pool)
      .await?
      .try_to_vm_spec()?;
    let vm_partial = VmSpecPartial {
      name: spec.name.to_owned().unwrap_or(vm.spec.name.clone()),
      image: old_spec.image,
      host_config: Some(spec.host_config.as_ref().map_or_else(
        || old_spec.host_config.clone(),
        |update| update.apply_to(&old_spec.host_config),
      )),
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
      mac_address: spec.mac_address.clone().or(old_spec.mac_address),
      cloud_init: spec.cloud_init.or(old_spec.cloud_init),
      cloud_init_user_data: merge_cloud_init_user_data(
        spec.cloud_init_user_data.clone(),
        old_spec.cloud_init_user_data,
      ),
      console: spec.console.or(old_spec.console),
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
    Self::put_spec_unlocked(pk, obj, state, owner).await
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn vm_cloud_init_patch_distinguishes_omitted_null_and_empty() {
    let current = Some(VmCloudInitUserData::Raw("#cloud-config\n".into()));
    assert_eq!(merge_cloud_init_user_data(None, current.clone()), current);
    assert_eq!(
      merge_cloud_init_user_data(
        Some(VmCloudInitUserData::Reset),
        current.clone()
      ),
      None
    );
    let empty = Some(VmCloudInitUserData::Raw(String::new()));
    assert_eq!(merge_cloud_init_user_data(empty.clone(), current), empty);
    assert_eq!(
      merge_cloud_init_user_data(Some(VmCloudInitUserData::Reset), None),
      None
    );
  }

  #[test]
  fn vm_mac_validation_rejects_multicast_zero_and_injected_values() {
    for value in [
      "01:00:00:00:00:01",
      "ff:ff:ff:ff:ff:ff",
      "00:00:00:00:00:00",
      "02:00:00:00:00",
      "2:00:00:00:00:01",
      "02:00:00:00:00:gg",
      "02:00:00:00:00:01\n",
      "02:00:00:00:00:01,model=e1000",
    ] {
      assert!(normalize_mac_address(value).is_err(), "{value:?}");
    }
    assert_eq!(
      normalize_mac_address("02:AB:CD:EF:00:01").unwrap(),
      "02:ab:cd:ef:00:01"
    );
  }

  #[test]
  fn vm_mac_allocation_preserves_identity_and_rejects_conflicts() {
    let empty = HashSet::new();
    let generated = select_mac_address(None, None, &empty).unwrap();
    assert!(generated.starts_with("02:"));
    assert_eq!(normalize_mac_address(&generated).unwrap(), generated);
    assert_eq!(
      select_mac_address(None, Some(&generated), &empty).unwrap(),
      generated
    );
    assert_eq!(
      select_mac_address(
        Some("02:AB:CD:EF:00:01"),
        Some("02:ab:cd:ef:00:01"),
        &empty
      )
      .unwrap(),
      "02:ab:cd:ef:00:01"
    );
    assert!(
      select_mac_address(
        Some("02:00:00:00:00:02"),
        Some("02:00:00:00:00:01"),
        &empty
      )
      .is_err()
    );
    let used = HashSet::from([generated.clone()]);
    assert!(select_mac_address(Some(&generated), None, &used).is_err());
    assert!(select_mac_address(None, Some(&generated), &used).is_err());
  }
}
