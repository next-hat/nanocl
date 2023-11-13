use ntex::web;
use diesel::prelude::*;

use nanocl_error::io::{IoResult, FromIo, IoError};

use nanocl_stubs::generic::GenericDelete;
use nanocl_stubs::job::{Job, JobPartial, JobUpdate};

use crate::utils;
use crate::models::{Pool, JobDbModel, JobUpdateDbModel};

pub async fn create(item: &JobPartial, pool: &Pool) -> IoResult<Job> {
  let mut data = serde_json::to_value(item)
    .map_err(|err| err.map_err_context(|| "JobPartial"))?;
  if let Some(meta) = data.as_object_mut() {
    meta.remove("Metadata");
  }
  let dbmodel = JobDbModel {
    key: item.name.clone(),
    created_at: chrono::Local::now().naive_local(),
    updated_at: chrono::Local::now().naive_local(),
    data,
    metadata: item.metadata.clone(),
  };
  let db_model =
    super::generic::insert_with_res::<_, _, JobDbModel>(dbmodel.clone(), pool)
      .await?;
  let job = db_model.into_job(item);
  Ok(job)
}

pub async fn delete_by_name(
  name: &str,
  pool: &Pool,
) -> IoResult<GenericDelete> {
  use crate::schema::jobs;
  let name = name.to_owned();
  super::generic::delete_by_id::<jobs::table, _>(name, pool).await
}

pub async fn update_by_name(
  name: &str,
  item: &JobUpdate,
  pool: &Pool,
) -> IoResult<Job> {
  use crate::schema::jobs;
  let name = name.to_owned();
  let dbmodel = JobUpdateDbModel {
    key: item.name.clone(),
    created_at: None,
    updated_at: Some(chrono::Local::now().naive_local()),
    data: Some(
      serde_json::to_value(item)
        .map_err(|err| err.map_err_context(|| "Job"))?,
    ),
    metadata: item.metadata.clone(),
  };
  let dbmodel =
    super::generic::update_by_id_with_res::<jobs::table, _, _, JobDbModel>(
      name, dbmodel, pool,
    )
    .await?;
  let item = dbmodel.serialize_data()?;
  let job = dbmodel.into_job(&item);
  Ok(job)
}

pub async fn list(pool: &Pool) -> IoResult<Vec<Job>> {
  use crate::schema::jobs;
  let pool = pool.clone();
  let items = web::block(move || {
    let mut conn = utils::store::get_pool_conn(&pool)?;
    let items: Vec<JobDbModel> = jobs::dsl::jobs
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

pub async fn find_by_name(name: &str, pool: &Pool) -> IoResult<Job> {
  use crate::schema::jobs;
  let name = name.to_owned();
  let db_model: JobDbModel =
    super::generic::find_by_id::<jobs::table, _, _>(name, pool).await?;
  let item = db_model.serialize_data()?;
  let job = db_model.into_job(&item);
  Ok(job)
}
