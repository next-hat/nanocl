use bollard_next::container::RemoveContainerOptions;
use futures_util::{StreamExt, stream::FuturesUnordered};

use nanocl_error::http::{HttpResult, HttpError};
use nanocl_stubs::job::{Job, JobPartial};

use crate::{
  utils,
  repositories::generic::*,
  models::{JobDb, ProcessDb},
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
    let job = JobDb::create_from(db_model, &state.pool)
      .await?
      .to_spec(obj);
    job
      .containers
      .iter()
      .map(|container| {
        let job_name = job.name.clone();
        async move {
          let mut container = container.clone();
          let mut labels = container.labels.clone().unwrap_or_default();
          labels.insert("io.nanocl.j".to_owned(), job_name.clone());
          container.labels = Some(labels);
          let short_id = utils::key::generate_short_id(6);
          let name = format!("{job_name}-{short_id}.j");
          utils::process::create(&name, "job", &job_name, container, state)
            .await?;
          Ok::<_, HttpError>(())
        }
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<(), HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, _>>()?;
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
    let processes = ProcessDb::read_by_kind_key(pk, &state.pool).await?;
    processes
      .into_iter()
      .map(|process| async move {
        utils::process::remove(
          &process.key,
          Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
          }),
          state,
        )
        .await
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<Result<(), HttpError>>>()
      .await
      .into_iter()
      .collect::<Result<Vec<_>, _>>()?;
    JobDb::del_by_pk(&job.name, &state.pool).await?;
    if job.schedule.is_some() {
      utils::job::remove_cron_rule(&job, state).await?;
    }
    Ok(job)
  }
}
