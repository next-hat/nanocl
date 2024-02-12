use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  system::{NativeEventAction, ObjPsStatusKind},
  process::ProcessKind,
};

use crate::{
  repositories::generic::*,
  models::{
    SystemState, VmDb, CargoDb, JobDb, JobUpdateDb, ObjPsStatusDb,
    ObjPsStatusUpdate,
  },
};

/// Represent a object that is treated as a process
/// That you can start, restart, stop, logs, etc.
pub trait ObjProcess {
  fn get_process_kind() -> ProcessKind;

  async fn _emit(
    kind_key: &str,
    action: NativeEventAction,
    state: &SystemState,
  ) -> HttpResult<()> {
    match Self::get_process_kind() {
      ProcessKind::Vm => {
        let vm = VmDb::transform_read_by_pk(kind_key, &state.pool).await?;
        state.emit_normal_native_action(&vm, action);
      }
      ProcessKind::Cargo => {
        let cargo =
          CargoDb::transform_read_by_pk(kind_key, &state.pool).await?;
        state.emit_normal_native_action(&cargo, action);
      }
      ProcessKind::Job => {
        JobDb::update_pk(
          kind_key,
          JobUpdateDb {
            updated_at: Some(chrono::Utc::now().naive_utc()),
          },
          &state.pool,
        )
        .await?;
        let job = JobDb::read_by_pk(kind_key, &state.pool)
          .await?
          .try_to_spec()?;
        state.emit_normal_native_action(&job, action);
      }
    }
    Ok(())
  }

  /// Emit a start process event to the system
  /// This will update the status of the process and emit a event
  /// So the system can take action for the group of process
  async fn emit_start(kind_key: &str, state: &SystemState) -> HttpResult<()> {
    let kind = Self::get_process_kind().to_string();
    log::debug!("starting {kind} {kind_key}");
    let current_status =
      ObjPsStatusDb::read_by_pk(kind_key, &state.pool).await?;
    if current_status.actual == ObjPsStatusKind::Start.to_string() {
      log::debug!("{kind} {kind_key} already running",);
      return Ok(());
    }
    let status_update = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Start.to_string()),
      prev_wanted: Some(current_status.wanted),
      actual: Some(ObjPsStatusKind::Starting.to_string()),
      prev_actual: Some(current_status.actual),
    };
    log::debug!("update status {kind} {kind_key}");
    ObjPsStatusDb::update_pk(kind_key, status_update, &state.pool).await?;
    Self::_emit(kind_key, NativeEventAction::Starting, state).await?;
    Ok(())
  }

  /// Emit a stop process event to the system
  /// This will update the status of the process and emit a event
  /// So the system can take action for the group of process
  async fn emit_stop(kind_key: &str, state: &SystemState) -> HttpResult<()> {
    let kind = Self::get_process_kind().to_string();
    log::debug!("stopping {kind} {kind_key}");
    let current_status =
      ObjPsStatusDb::read_by_pk(kind_key, &state.pool).await?;
    if current_status.actual == ObjPsStatusKind::Stop.to_string() {
      log::debug!("{kind} {kind_key} already stopped",);
      return Ok(());
    }
    let status_update = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Stop.to_string()),
      prev_wanted: Some(current_status.wanted),
      actual: Some(ObjPsStatusKind::Stopping.to_string()),
      prev_actual: Some(current_status.actual),
    };
    log::debug!("update status {kind} {kind_key}");
    ObjPsStatusDb::update_pk(kind_key, status_update, &state.pool).await?;
    Self::_emit(kind_key, NativeEventAction::Stopping, state).await?;
    Ok(())
  }
}
