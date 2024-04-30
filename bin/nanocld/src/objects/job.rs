use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  job::{Job, JobPartial, JobInspect},
  system::{ObjPsStatusPartial, ObjPsStatusKind, NativeEventAction},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{JobDb, ObjPsStatusDb, ObjPsStatusUpdate, ProcessDb},
};

use super::generic::*;

impl ObjCreate for JobDb {
  type ObjCreateIn = JobPartial;
  type ObjCreateOut = Job;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let db_model = JobDb::try_from_partial(obj)?;
    let status = ObjPsStatusPartial {
      key: obj.name.clone(),
      wanted: ObjPsStatusKind::Create,
      prev_wanted: ObjPsStatusKind::Create,
      actual: ObjPsStatusKind::Create,
      prev_actual: ObjPsStatusKind::Create,
    };
    let status = ObjPsStatusDb::create_from(status, &state.inner.pool).await?;
    let job = JobDb::create_from(db_model, &state.inner.pool)
      .await?
      .try_to_spec(&status)?;
    if let Some(schedule) = &job.schedule {
      utils::cron::add_cron_rule(&job, schedule, state).await?;
    }
    Ok(job)
  }
}

impl ObjDelByPk for JobDb {
  type ObjDelOpts = ();
  type ObjDelOut = Job;

  fn get_del_event() -> NativeEventAction {
    NativeEventAction::Destroying
  }

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let job = JobDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let status = ObjPsStatusDb::read_by_pk(pk, &state.inner.pool).await?;
    let new_status = ObjPsStatusUpdate {
      wanted: Some(ObjPsStatusKind::Destroy.to_string()),
      prev_wanted: Some(status.wanted),
      actual: Some(ObjPsStatusKind::Destroying.to_string()),
      prev_actual: Some(status.actual),
    };
    ObjPsStatusDb::update_pk(pk, new_status, &state.inner.pool).await?;
    Ok(job)
  }
}

impl ObjInspectByPk for JobDb {
  type ObjInspectOut = JobInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let job = JobDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let instances = ProcessDb::read_by_kind_key(pk, &state.inner.pool).await?;
    let (instance_total, instance_failed, instance_success, instance_running) =
      utils::container::count_status(&instances);
    let status =
      ObjPsStatusDb::read_by_pk(&job.name, &state.inner.pool).await?;
    let job_inspect = JobInspect {
      status: status
        .try_into()
        .map_err(HttpError::internal_server_error)?,
      spec: job,
      instance_total,
      instance_success,
      instance_running,
      instance_failed,
      instances,
    };
    Ok(job_inspect)
  }
}
