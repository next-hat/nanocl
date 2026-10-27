use nanocl_error::http::HttpResult;
use nanocl_stubs::{
  job::{Job, JobInspect},
  system::NativeEventAction,
};

use crate::{
  models::{JobDb, JobObjCreateIn},
  repositories::generic::*,
  utils,
};

use super::generic::*;

impl ObjCreate for JobDb {
  type ObjCreateIn = JobObjCreateIn;
  type ObjCreateOut = Job;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    let job =
      JobDb::create_with_spec(&obj.spec, &obj.version, &state.inner.pool)
        .await?;
    if let Some(schedule) = &job.spec.schedule {
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
    crate::tasks::job::delete(pk, state).await
  }
}

impl ObjInspectByPk for JobDb {
  type ObjInspectOut = JobInspect;

  async fn inspect_obj_by_pk(
    pk: &str,
    state: &crate::models::SystemState,
  ) -> HttpResult<Self::ObjInspectOut> {
    let job = JobDb::transform_read_by_pk(pk, &state.inner.pool).await?;
    let instances = JobDb::current_processes(&job, &state.inner.pool).await?;
    let (instance_total, instance_failed, instance_success, instance_running) =
      JobDb::current_counts(&job, &state.inner.pool).await?;
    let job_inspect = JobInspect {
      created_at: job.created_at,
      updated_at: job.updated_at,
      status: job.status,
      incarnation_key: job.incarnation_key,
      spec_key: job.spec_key,
      current_run_key: job.current_run_key,
      spec: job.spec,
      instance_total,
      instance_success,
      instance_running,
      instance_failed,
      instances,
    };
    Ok(job_inspect)
  }
}
