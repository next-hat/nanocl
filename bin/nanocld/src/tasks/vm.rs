use nanocl_error::io::IoError;
use nanocl_stubs::{process::ProcessKind, system::NativeEventAction};

use crate::{
  models::{ProcessDb, SystemState, VmDb, VmImageDb},
  repositories::generic::*,
  utils,
};

use super::generic::*;

// impl ObjTask for VmDb {}

impl ObjTaskStart for VmDb {
  fn create_start_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let image =
        VmImageDb::read_by_pk(&vm.spec.disk.image, &state.inner.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&vm.spec.vm_key, &state.inner.pool).await?;
      if processes.is_empty() {
        utils::container::create_vm_instance(&vm, &image, true, &state).await?;
      }
      utils::container::start_instances(
        &vm.spec.vm_key,
        &ProcessKind::Vm,
        &state,
      )
      .await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskStop for VmDb {
  fn create_stop_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      utils::container::stop_instances(&key, &ProcessKind::Vm, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskDelete for VmDb {
  fn create_delete_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let processes =
        ProcessDb::read_by_kind_key(&key, &state.inner.pool).await?;
      utils::container::delete_instances(
        &processes
          .into_iter()
          .map(|p| p.key)
          .collect::<Vec<String>>(),
        &state,
      )
      .await?;
      utils::vm_image::delete_by_pk(&vm.spec.disk.image, &state).await?;
      VmDb::clear_by_pk(&vm.spec.vm_key, &state.inner.pool).await?;
      state
        .emit_normal_native_action_sync(&vm, NativeEventAction::Destroy)
        .await;
      Ok::<_, IoError>(())
    })
  }
}

impl ObjTaskUpdate for VmDb {
  fn create_update_task(key: &str, state: &SystemState) -> ObjTaskFuture {
    let key = key.to_owned();
    let state = state.clone();
    Box::pin(async move {
      let vm = VmDb::transform_read_by_pk(&key, &state.inner.pool).await?;
      let container_name = format!("{}.v", &vm.spec.vm_key);
      let image =
        VmImageDb::read_by_pk(&vm.spec.disk.image, &state.inner.pool).await?;
      utils::container::delete_instances(&[container_name], &state).await?;
      utils::container::create_vm_instance(&vm, &image, false, &state).await?;
      utils::container::start_instances(&key, &ProcessKind::Vm, &state).await?;
      Ok::<_, IoError>(())
    })
  }
}
