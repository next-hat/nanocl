use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::job::{Job, JobPartial};

use crate::utils;
use crate::models::{Pool, JobDb, JobUpdateDb};

/// ## Create
///
/// Create a job [job](Job) from a [job partial](JobPartial)
///
/// ## Arguments
///
/// * [item](JobPartial) The job partial
/// * [pool](Pool) The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Job](Job)
///
pub(crate) async fn create(item: &JobPartial, pool: &Pool) -> IoResult<Job> {
  let mut data = serde_json::to_value(item)
    .map_err(|err| err.map_err_context(|| "JobPartial"))?;
  if let Some(meta) = data.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = JobDb {
    key: item.name.clone(),
    created_at: chrono::Local::now().naive_utc(),
    updated_at: chrono::Local::now().naive_utc(),
    data,
    metadata: item.metadata.clone(),
  };
  let db_model =
    super::generic::insert_with_res::<_, _, JobDb>(dbmodel.clone(), pool)
      .await?;
  let job = db_model.into_job(item);
  Ok(job)
}

/// ## Delete by name
///
/// Delete a job by it's name
///
/// ## Arguments
///
/// * [name](str) The name of the job to delete
/// * [pool](Pool) The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [GenericDelete](GenericDelete)
///
pub(crate) async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::jobs;
  let name = name.to_owned();
  super::generic::delete_by_id::<jobs::table, _>(name, pool).await
}

/// ## List
///
/// List all jobs
///
/// ## Arguments
///
/// * [pool](Pool) The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Vec](Vec) of [Job](Job)
///
pub(crate) async fn list(pool: &Pool) -> IoResult<Vec<Job>> {
  use crate::schema::jobs;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items: Vec<JobDb> = jobs::dsl::jobs
      .order(jobs::dsl::created_at.desc())
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "Job"))?;
    Ok::<_, IoError>(items)
  })
  .await?;
  let items = items
    .into_iter()
    .map(|item| {
      let data = item.serialize_data()?;
      Ok::<_, IoError>(item.into_job(&data))
    })
    .collect::<Result<Vec<_>, IoError>>()?;
  Ok(items)
}

/// ## Find by name
///
/// Find a job by it's name
///
/// ## Arguments
///
/// * [name](str) The name of the job to find
/// * [pool](Pool) The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Job](Job)
///
pub(crate) async fn find_by_name(name: &str, pool: &Pool) -> IoResult<Job> {
  use crate::schema::jobs;
  let name = name.to_owned();
  let db_model: JobDb =
    super::generic::find_by_id::<jobs::table, _, _>(name, pool).await?;
  let item = db_model.serialize_data()?;
  let job = db_model.into_job(&item);
  Ok(job)
}

/// ## Update by name
///
/// Update a job by it's name
///
/// ## Arguments
///
/// * [name](str) The name of the job to update
/// * [data](JobUpdateDb) The data to update
/// * [pool](Pool) The database pool
///
/// ## Return
///
/// [IoResult](IoResult) containing a [Job](Job)
///
pub(crate) async fn update_by_name(
  name: &str,
  data: &JobUpdateDb,
  pool: &Pool,
) -> IoResult<Job> {
  use crate::schema::jobs;
  let name = name.to_owned();
  super::generic::update_by_id::<jobs::table, _, _>(
    name.clone(),
    data.clone(),
    pool,
  )
  .await?;
  let db_model: JobDb =
    super::generic::find_by_id::<jobs::table, _, _>(name, pool).await?;
  let job = db_model.serialize_data()?;
  let job = db_model.into_job(&job);
  Ok(job)
}
