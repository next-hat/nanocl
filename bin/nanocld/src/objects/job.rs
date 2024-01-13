use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  job::{Job, JobPartial, JobInspect},
  process::ProcessKind,
  system::{ObjPsStatusPartial, ObjPsStatusKind},
};

use crate::{
  utils,
  repositories::generic::*,
  models::{JobDb, ProcessDb, ObjPsStatusDb},
};

use super::generic::*;

impl ObjProcess for JobDb {
  fn get_process_kind() -> ProcessKind {
    ProcessKind::Job
  }
}

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
      wanted: ObjPsStatusKind::Created,
      prev_wanted: ObjPsStatusKind::Created,
      actual: ObjPsStatusKind::Created,
      prev_actual: ObjPsStatusKind::Created,
    };
    ObjPsStatusDb::create_from(status, &state.pool).await?;
    let job = JobDb::create_from(db_model, &state.pool)
      .await?
      .to_spec(obj);
    if let Some(schedule) = &job.schedule {
      utils::job::add_cron_rule(&job, schedule, state).await?;
    }
    Ok(job)
  }
}

impl ObjDelByPk for JobDb {
  type ObjDelOpts = ();
  type ObjDelOut = Job;

  async fn fn_del_obj_by_pk(
    pk: &str,
    _opts: &Self::ObjDelOpts,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let job = JobDb::read_by_pk(pk, &state.pool).await?.try_to_spec()?;
    Ok(job)
  }
}

impl ObjInspectByPk for JobDb {
  type ObjInspectOut = JobInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let job = JobDb::read_by_pk(pk, &state.pool).await?.try_to_spec()?;
    let instances = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    let (instance_total, instance_failed, instance_success, instance_running) =
      utils::process::count_status(&instances);
    let job_inspect = JobInspect {
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
