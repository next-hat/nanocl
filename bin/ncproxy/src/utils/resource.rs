use futures::{stream::FuturesUnordered, StreamExt};
use nanocl_error::io::{IoError, IoResult, FromIo};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericClause},
    proxy::ResourceProxyRule,
    resource::{Resource, ResourcePartial},
  },
};

use crate::{vars, models::SystemStateRef};

pub async fn list_by_secret(
  name: &str,
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  let filter = GenericFilter::new()
    .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()))
    .r#where(
      "data",
      GenericClause::Contains(
        serde_json::json!({ "Rules": [ { "Ssl": name }  ] }),
      ),
    );
  let resources = client.list_resource(Some(&filter)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list resources from nanocl daemon")
  })?;
  if resources.is_empty() {
    return Err(IoError::not_found(
      "Resource",
      &format!("No resources found matching secret {name}"),
    ));
  }
  Ok(resources)
}

pub(crate) async fn list_by_cargo(
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  let namespace = namespace.unwrap_or("global".into());
  let target_key = format!("{name}.{namespace}.c");
  let filter = GenericFilter::new()
  .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()))
  .r#where(
    "data",
    GenericClause::Contains(
      serde_json::json!({ "Rules": [ { "Locations": [ { "Target": { "Key": target_key } } ] }  ] }),
    ),
  );
  let http_resources =
    client.list_resource(Some(&filter)).await.map_err(|err| {
      err.map_err_context(|| "Unable to list resources from nanocl daemon")
    })?;
  let filter = GenericFilter::new()
  .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()))
  .r#where(
    "data",
    GenericClause::Contains(
      serde_json::json!({ "Rules": [ {  "Target": { "Key": target_key } } ] }),
    ),
  );
  let stream_resources =
    client.list_resource(Some(&filter)).await.map_err(|err| {
      err.map_err_context(|| "Unable to list resources from nanocl daemon")
    })?;
  let resources = http_resources
    .into_iter()
    .chain(stream_resources.into_iter())
    .collect::<Vec<nanocld_client::stubs::resource::Resource>>();
  if resources.is_empty() {
    return Err(IoError::not_found(
      "Resource",
      &format!("No resources found matching cargo {target_key}"),
    ));
  }
  Ok(resources)
}

pub(crate) fn serialize(
  data: &serde_json::Value,
) -> IoResult<ResourceProxyRule> {
  let resource = serde_json::from_value::<ResourceProxyRule>(data.clone())
    .map_err(|err| {
      err.map_err_context(|| "Unable to serialize ResourceProxyRule")
    })?;
  Ok(resource)
}

pub(crate) async fn update_rules(
  resources: &[Resource],
  state: &SystemStateRef,
) -> IoResult<()> {
  resources
    .iter()
    .map(|resource| async move {
      let resource: ResourcePartial = resource.clone().into();
      let rule = serialize(&resource.data)?;
      super::nginx::add_rule(&resource.name, &rule, state).await?;
      Ok::<_, IoError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, IoError>>()?;
  Ok(())
}
