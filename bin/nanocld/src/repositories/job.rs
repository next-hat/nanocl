use std::sync::Arc;

use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::job::{Job, JobPartial};

use crate::utils;
use crate::models::{Pool, JobDb, JobUpdateDb, FromSpec};

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
  let db_model =
    JobDb::try_from_spec_partial(&item.name, crate::version::VERSION, item)?;
  let db_model =
    super::generic::insert_with_res::<_, _, JobDb>(db_model, pool).await?;
  let item = db_model.to_spec(item);
  Ok(item)
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
  let pool = Arc::clone(pool);
  let db_models = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let db_models: Vec<JobDb> = jobs::dsl::jobs
      .order(jobs::dsl::created_at.desc())
      .get_results(&mut conn)
      .map_err(|err| err.map_err_context(|| "Job"))?;
    Ok::<_, IoError>(db_models)
  })
  .await?;
  let items = db_models
    .into_iter()
    .map(|db_model| db_model.try_to_spec())
    .collect::<IoResult<Vec<_>>>()?;
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
  let item = db_model.try_to_spec()?;
  Ok(item)
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
  let item = db_model.try_to_spec()?;
  Ok(item)
}
