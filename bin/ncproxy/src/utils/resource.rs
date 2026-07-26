use futures::{StreamExt, stream::FuturesUnordered};
use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericClause, GenericFilter},
    proxy::ResourceProxyRule,
    resource::{Resource, ResourcePartial},
  },
};

use crate::{models::SystemStateRef, vars};

fn workload_target_key(
  name: &str,
  namespace: &str,
  kind: &str,
) -> IoResult<String> {
  let key =
    nanocld_client::stubs::resource_key::ResourceKey::new(name, namespace)
      .map_err(|err| {
        IoError::invalid_input("Workload key", &err.to_string())
      })?;
  Ok(format!("{key}.{kind}"))
}

pub(crate) async fn list_all(
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  let filter = GenericFilter::new()
    .limit(10_000)
    .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()));
  let resources = client.list_resource(Some(&filter)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list ncproxy resources from nanocld")
  })?;
  Ok(resources)
}

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

async fn list_by_workload(
  name: &str,
  namespace: Option<String>,
  kind: &str,
  kind_name: &str,
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  let namespace = namespace.unwrap_or("global".into());
  let target_key = workload_target_key(name, &namespace, kind)?;
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
    .chain(stream_resources)
    .collect::<Vec<nanocld_client::stubs::resource::Resource>>();
  if resources.is_empty() {
    return Err(IoError::not_found(
      "Resource",
      &format!("No resources found matching {kind_name} {target_key}"),
    ));
  }
  Ok(resources)
}

pub(crate) async fn list_by_cargo(
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  list_by_workload(name, namespace, "c", "cargo", client).await
}

pub(crate) async fn list_by_vm(
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> IoResult<Vec<Resource>> {
  list_by_workload(name, namespace, "v", "vm", client).await
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

/// Rebuild the complete generated configuration from committed nanocld state.
///
/// The nginx layer clears the managed directories transactionally, which also
/// removes resources deleted while this controller was offline.
pub(crate) async fn rebuild_all_rules(state: &SystemStateRef) -> IoResult<()> {
  let resources = list_all(&state.client).await?;
  let mut rules = resources
    .into_iter()
    .map(|resource| {
      let resource: ResourcePartial = resource.into();
      let rule = serialize(&resource.data)?;
      Ok::<_, IoError>((resource.name, rule))
    })
    .collect::<IoResult<Vec<_>>>()?;
  rules.sort_by(|left, right| left.0.cmp(&right.0));
  super::nginx::rebuild_rules(&rules, state).await?;
  state.event_emitter.emit_reload().await;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::workload_target_key;

  #[test]
  fn workload_target_keys_use_the_expected_kind_suffix() {
    assert_eq!(
      workload_target_key("api", "global", "c").unwrap(),
      "api.global.c"
    );
    assert_eq!(
      workload_target_key("db", "private", "v").unwrap(),
      "db.private.v"
    );
  }
}
