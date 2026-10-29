use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  job::{Job, JobInspect, JobPartial},
  system::NativeEventAction,
};

use crate::{
  models::{JobDb, JobObjCreateIn, SystemState},
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
    // A cancelled run can briefly reacquire this lock after deletion completes.
    // Wait for its final observation before admitting the replacement.
    let serialized = std::sync::Arc::new(
      utils::container::job::lifecycle_lock(&obj.spec.name)
        .lock_owned()
        .await,
    );
    Self::create_with_lifecycle_lock(obj, state, serialized).await
  }
}

impl JobDb {
  /// The caller may retain admission across deletion and this creation.
  async fn create_with_lifecycle_lock(
    obj: &JobObjCreateIn,
    state: &SystemState,
    serialized: std::sync::Arc<tokio::sync::OwnedMutexGuard<()>>,
  ) -> HttpResult<Job> {
    let spec = obj.spec.clone();
    let version = obj.version.clone();
    let state = state.clone();
    let (reply, response) = tokio::sync::oneshot::channel();
    // Once creation begins, its owner keeps registration ordered with
    // start/delete even if the request stops waiting for the response.
    ntex::rt::spawn(async move {
      let _serialized = serialized;
      let result = async {
        let job =
          JobDb::create_with_spec(&spec, &version, &state.inner.pool).await?;
        if let Some(schedule) = &job.spec.schedule
          && let Err(error) =
            utils::cron::add_cron_rule(&job, schedule, &state).await
        {
          // Installation may have succeeded before its result was lost. Try
          // to remove its exact incarnation-scoped rule before undoing the
          // unpublished database creation, retaining admission throughout.
          let cron_cleanup = utils::cron::remove_cron_rule(&job, &state).await;
          let rollback = JobDb::rollback_creation(
            &job,
            _serialized.clone(),
            &state.inner.pool,
          )
          .await;
          let mut error: HttpError = error.into();
          match rollback {
            Ok(()) => {
              error.msg.push_str("; failed Job creation was rolled back");
            }
            Err(rollback) => {
              error.msg.push_str(&format!(
                "; Job creation rollback could not be confirmed: {rollback}; inspect the Job before retrying"
              ));
            }
          }
          if let Err(cleanup) = cron_cleanup {
            // Every cron request carries the original incarnation, so a
            // residual rule cannot start a replacement with the same name.
            error.msg.push_str(&format!(
              "; cron rule cleanup could not be confirmed: {cleanup}; any residual rule remains bound to the original Job incarnation"
            ));
          }
          return Err(error);
        }
        Ok::<_, HttpError>(job)
      }
      .await;
      // Release admission before the caller can immediately start this Job.
      drop(_serialized);
      let _ = reply.send(result);
    });
    response.await.map_err(|_| {
      HttpError::internal_server_error(
        "Job creation owner stopped before replying",
      )
    })?
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

impl JobDb {
  /// Restore a declaration through the existing delete/create lifecycle.
  /// This deliberately does not execute the restored Job.
  pub(crate) async fn revert_to_history(
    name: &str,
    history_key: uuid::Uuid,
    version: &str,
    state: &SystemState,
  ) -> HttpResult<Job> {
    let current = Self::transform_read_by_pk(name, &state.inner.pool).await?;
    let stored =
      Self::history_spec(name, history_key, &state.inner.pool).await?;
    if stored.name != name {
      return Err(HttpError::bad_request(
        "Cannot revert a Job to a history with another resource name",
      ));
    }
    let spec = JobPartial::from(stored);
    Self::validate_partial(&spec)?;
    let obj = JobObjCreateIn {
      spec,
      version: version.to_owned(),
    };
    // Reserve this name through both lifecycle phases and their events.
    let serialized = std::sync::Arc::new(
      utils::container::job::lifecycle_lock(name)
        .lock_owned()
        .await,
    );
    let state = state.clone();
    let (reply, response) = tokio::sync::oneshot::channel();
    // Retain ownership of the whole replacement if the HTTP caller disconnects.
    ntex::rt::spawn(async move {
      let result = async {
        crate::tasks::job::delete_for_replacement_with_lifecycle_lock(
          &current,
          &state,
          serialized.clone(),
        )
        .await?;
        let job = Self::create_with_lifecycle_lock(&obj, &state, serialized.clone()).await.map_err(|mut error| {
          error.msg = format!(
            "Job {} was removed, but its restored configuration could not be created: {}; retained configurations and runs remain available",
            current.spec.name, error.msg,
          );
          log::warn!("Job {} revert failed after deletion: {error}", current.spec.name);
          error
        })?;
        state.emit_normal_native_action_sync(&job, NativeEventAction::Create).await;
        Ok(job)
      }
      .await;
      drop(serialized);
      let _ = reply.send(result);
    });
    response.await.map_err(|_| {
      HttpError::internal_server_error(
        "Job revert owner stopped before replying; inspect the Job before retrying",
      )
    })?
  }
}
