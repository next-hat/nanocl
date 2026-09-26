use std::collections::HashMap;

use nanocl_error::io::IoResult;
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericClause, GenericFilter, GenericFilterNsp},
    system::Event,
  },
};

use crate::models::CompletionKind;

const PAGE_SIZE: usize = 100;

/// Keep shell input literal when it becomes a SQL LIKE pattern.
fn prefix_pattern(current: &str) -> String {
  let mut pattern = String::with_capacity(current.len() + 1);
  for character in current.chars() {
    if matches!(character, '\\' | '%' | '_') {
      pattern.push('\\');
    }
    pattern.push(character);
  }
  pattern.push('%');
  pattern
}

fn prefix_filter(column: &str, current: &str) -> GenericFilter {
  let mut filter = GenericFilter::new()
    .r#where(column, GenericClause::Like(prefix_pattern(current)))
    .limit(PAGE_SIZE);
  filter.order_by = Some(vec![format!("{column} asc")]);
  filter
}

fn process_filter(current: &str) -> GenericFilter {
  let mut filter = prefix_filter("name", current);
  // GenericWhere combines the normal conditions with each `or` group.
  // This therefore selects matching process names OR full process IDs.
  filter.r#where.as_mut().unwrap().or = Some(vec![HashMap::from([(
    "key".to_owned(),
    GenericClause::Like(prefix_pattern(current)),
  )])]);
  // Process IDs are unique, giving the limited result a stable order.
  filter.order_by = Some(vec!["key asc".to_owned()]);
  filter
}

pub(super) async fn candidates(
  client: &NanocldClient,
  kind: CompletionKind,
  current: &str,
  selected: Option<&str>,
) -> IoResult<Vec<String>> {
  let names = match kind {
    CompletionKind::Cargo => {
      let query = GenericFilterNsp {
        filter: Some(prefix_filter("key", current)),
        namespace: None,
      };
      client
        .list_cargo(Some(&query))
        .await?
        .into_iter()
        .map(|cargo| cargo.spec.cargo_key)
        .collect()
    }
    CompletionKind::Vm => {
      let query = GenericFilterNsp {
        filter: Some(prefix_filter("key", current)),
        namespace: None,
      };
      client
        .list_vm(Some(&query))
        .await?
        .into_iter()
        .map(|vm| vm.spec.vm_key)
        .collect()
    }
    CompletionKind::Job => client
      .list_job(Some(&prefix_filter("key", current)))
      .await?
      .into_iter()
      .map(|job| job.spec.name)
      .collect(),
    CompletionKind::Resource => client
      .list_resource(Some(&prefix_filter("key", current)))
      .await?
      .into_iter()
      .map(|resource| resource.spec.resource_key)
      .collect(),
    CompletionKind::Namespace => client
      .list_namespace(Some(&prefix_filter("name", current)))
      .await?
      .into_iter()
      .map(|namespace| namespace.name)
      .collect(),
    CompletionKind::Secret => client
      .list_secret(Some(&prefix_filter("key", current)))
      .await?
      .into_iter()
      .map(|secret| secret.name)
      .collect(),
    CompletionKind::Process => client
      .list_process(Some(&process_filter(current)))
      .await?
      .into_iter()
      .flat_map(|process| [process.name, process.key])
      .collect(),
    CompletionKind::CargoContainer => {
      let Some(key) = selected.filter(|key| !key.is_empty()) else {
        return Ok(Vec::new());
      };
      let cargo = client.inspect_cargo(key).await?;
      cargo
        .spec
        .containers
        .into_iter()
        .map(|container| container.name)
        .collect()
    }
    // History endpoints currently expose the latest 100 revisions and do not
    // accept pagination or prefix filters. Match within that existing window.
    CompletionKind::CargoHistory => {
      let Some(key) = selected.filter(|key| !key.is_empty()) else {
        return Ok(Vec::new());
      };
      client
        .list_history_cargo(key)
        .await?
        .into_iter()
        .map(|revision| revision.key.to_string())
        .collect()
    }
    // The same latest-100 endpoint limit applies to resource revisions.
    CompletionKind::ResourceHistory => {
      let Some(key) = selected.filter(|key| !key.is_empty()) else {
        return Ok(Vec::new());
      };
      client
        .list_history_resource(key)
        .await?
        .into_iter()
        .map(|revision| revision.key.to_string())
        .collect()
    }
    CompletionKind::Event | CompletionKind::Metric => {
      uuid_candidates(client, kind, current).await?
    }
    // Contexts come from the local configuration, before opening a client.
    CompletionKind::Context => Vec::new(),
  };
  Ok(names)
}

/// UUID columns do not support LIKE in the existing API. Page deterministically
/// until enough matching IDs are found; the caller bounds the entire operation
/// with the completion deadline.
async fn uuid_candidates(
  client: &NanocldClient,
  kind: CompletionKind,
  current: &str,
) -> IoResult<Vec<String>> {
  let mut candidates = Vec::new();
  let mut offset = 0;
  loop {
    let filter = GenericFilter {
      limit: Some(PAGE_SIZE),
      offset: Some(offset),
      order_by: Some(vec!["key asc".to_owned()]),
      ..Default::default()
    };
    let keys = match kind {
      CompletionKind::Event => {
        let query = NanocldClient::convert_query(Some(&filter))?;
        let response = client.send_get("/events", Some(query)).await?;
        NanocldClient::res_json::<Vec<Event>>(response)
          .await?
          .into_iter()
          .map(|event| event.key.to_string())
          .collect::<Vec<_>>()
      }
      CompletionKind::Metric => client
        .list_metric(Some(&filter))
        .await?
        .into_iter()
        .map(|metric| metric.key.to_string())
        .collect(),
      _ => return Ok(Vec::new()),
    };
    let page_len = keys.len();
    candidates.extend(keys.into_iter().filter(|key| key.starts_with(current)));
    if candidates.len() >= PAGE_SIZE || page_len < PAGE_SIZE {
      candidates.truncate(PAGE_SIZE);
      return Ok(candidates);
    }
    offset += page_len;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn prefix_filter_escapes_sql_wildcards_and_orders_before_limiting() {
    let filter = prefix_filter("key", "default.api_%\\backup");
    let value = serde_json::to_value(filter).unwrap();
    assert_eq!(
      value["where"]["key"],
      serde_json::json!({"like": "default.api\\_\\%\\\\backup%"})
    );
    assert_eq!(value["limit"], 100);
    assert_eq!(value["order_by"], serde_json::json!(["key asc"]));
  }

  #[test]
  fn process_filter_matches_names_or_ids_without_an_unfiltered_branch() {
    let value = serde_json::to_value(process_filter("api_")).unwrap();
    assert_eq!(
      value["where"],
      serde_json::json!({
        "name": {"like": "api\\_%"},
        "or": [{"key": {"like": "api\\_%"}}]
      })
    );
    assert_eq!(value["order_by"], serde_json::json!(["key asc"]));
    assert_eq!(value["limit"], 100);
  }
}
