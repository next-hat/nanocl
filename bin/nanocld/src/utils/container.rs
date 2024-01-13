use bollard_next::container::{
  Config, CreateContainerOptions, InspectContainerOptions,
};
use nanocl_error::{
  http::{HttpResult, HttpError},
  io::FromIo,
};
use nanocl_stubs::process::{Process, ProcessPartial, ProcessKind};

use crate::{
  repositories::generic::*,
  models::{SystemState, ProcessDb},
};

pub async fn create_process(
  kind: &ProcessKind,
  name: &str,
  kind_key: &str,
  item: Config,
  state: &SystemState,
) -> HttpResult<Process> {
  let mut config = item.clone();
  let mut labels = item.labels.to_owned().unwrap_or_default();
  labels.insert("io.nanocl".to_owned(), "enabled".to_owned());
  labels.insert("io.nanocl.kind".to_owned(), kind.to_string());
  config.labels = Some(labels);
  let res = state
    .docker_api
    .create_container(
      Some(CreateContainerOptions {
        name,
        ..Default::default()
      }),
      config,
    )
    .await?;
  let inspect = state
    .docker_api
    .inspect_container(&res.id, None::<InspectContainerOptions>)
    .await?;
  let created_at = inspect.created.clone().unwrap_or_default();
  let new_instance = ProcessPartial {
    key: res.id,
    name: name.to_owned(),
    kind: kind.clone(),
    data: serde_json::to_value(&inspect)
      .map_err(|err| err.map_err_context(|| "CreateProcess"))?,
    node_key: state.config.hostname.clone(),
    kind_key: kind_key.to_owned(),
    created_at: Some(
      chrono::NaiveDateTime::parse_from_str(
        &created_at,
        "%Y-%m-%dT%H:%M:%S%.fZ",
      )
      .map_err(|err| {
        HttpError::internal_server_error(format!("Unable to parse date {err}"))
      })?,
    ),
  };
  let process = ProcessDb::create_from(&new_instance, &state.pool).await?;
  Process::try_from(process).map_err(HttpError::from)
}
