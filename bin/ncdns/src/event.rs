use std::{collections::BTreeMap, time::Duration};

use futures::StreamExt;
use ntex::rt;

use nanocl_error::io::{FromIo, IoResult};
use nanocl_utils::versioning;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource_kind::{
  ResourceKindPartial, ResourceKindSpec,
};
use nanocld_client::stubs::system::{
  Event, EventActorKind, EventKind, NativeEventAction,
};
use nanocld_client::stubs::{
  dns::ResourceDnsRule,
  generic::{GenericClause, GenericFilter},
};

use crate::{models::SystemState, utils, vars};

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub(super) async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formatted_version = versioning::format_version(vars::VERSION);
  let resource_kind = ResourceKindPartial {
    name: vars::RULE_KEY.to_owned(),
    version: format!("v{formatted_version}"),
    metadata: None,
    data: ResourceKindSpec {
      schema: None,
      url: Some("unix:///run/nanocl/dns.sock".to_owned()),
    },
  };
  if client
    .inspect_resource_kind_version(&resource_kind.name, &resource_kind.version)
    .await
    .is_ok()
  {
    return Ok(());
  }
  client.create_resource_kind(&resource_kind).await?;
  Ok(())
}

async fn load_rules(
  client: &NanocldClient,
) -> IoResult<BTreeMap<String, ResourceDnsRule>> {
  const PAGE_SIZE: usize = 100;
  let mut rules = BTreeMap::new();
  let mut last_key = None;
  loop {
    let mut query = GenericFilter::new()
      .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()))
      .limit(PAGE_SIZE);
    query.order_by = Some(vec!["key asc".to_owned()]);
    if let Some(key) = last_key {
      query = query.r#where("key", GenericClause::Gt(key));
    }
    let page = client.list_resource(Some(&query)).await?;
    let count = page.len();
    last_key = page
      .last()
      .map(|resource| resource.spec.resource_key.clone());
    for resource in page {
      let rule =
        serde_json::from_value(resource.spec.data).map_err(|error| {
          error.map_err_context(|| "DNS recovery declaration")
        })?;
      rules.insert(resource.spec.resource_key, rule);
    }
    if count < PAGE_SIZE {
      return Ok(rules);
    }
  }
}

async fn recover_rules(state: &SystemState) -> IoResult<()> {
  let needs_bootstrap = !state.rules.lock().await.bootstrapped;
  if needs_bootstrap {
    // Callbacks may complete while the inventory is fetched. Their accepted
    // declarations and deletion markers take precedence over this snapshot.
    let rules = load_rules(&state.client).await?;
    state.rules.lock().await.bootstrap(rules);
  }
  // Keep callbacks ordered through resolution and application, not just the
  // final filesystem update. Reconnects reuse accepted declarations locally.
  let rules = state.rules.lock().await;
  let mut failure = None;
  for (key, rule) in &rules.declarations {
    let result = async {
      let scope = utils::resolve_rule(rule, &state.client).await?;
      state.dnsmasq.apply_rule(key, scope).await
    }
    .await;
    if let Err(error) = result {
      log::warn!("event::recovery: rule {key}: {error}");
      failure = Some(error);
    }
  }
  let keys = rules.declarations.keys().cloned().collect();
  if let Err(error) = state.dnsmasq.remove_stale_rules(&keys).await {
    log::warn!("event::recovery: stale rules: {error}");
    failure = Some(error);
  }
  match failure {
    Some(error) => Err(error),
    None => Ok(()),
  }
}

fn is_state_sync(event: &Event) -> bool {
  event.kind == EventKind::Normal
    && event.action == NativeEventAction::Update.to_string()
    && event.reason == "state_sync"
    && event.actor.as_ref().is_some_and(|actor| {
      actor.kind == EventActorKind::Process && actor.key.is_none()
    })
}

async fn ensure_loop(state: &SystemState) {
  let client = &state.client;
  loop {
    if let Err(err) = ensure_self_config(client).await {
      log::warn!("event::loop: unable to ensure resource kind: {err}");
      ntex::time::sleep(RETRY_DELAY).await;
      continue;
    }
    log::info!("event::loop: subscribing to nanocld events");
    match client.watch_events(None).await {
      Err(err) => {
        log::warn!("event::loop: {err}");
      }
      Ok(mut stream) => {
        log::info!("event::loop: subscribed to nanocld events");
        if let Err(error) = recover_rules(state).await {
          log::warn!("event::loop: DNS recovery failed: {error}");
          ntex::time::sleep(RETRY_DELAY).await;
          continue;
        }
        while let Some(event) = stream.next().await {
          match event {
            Ok(event) if is_state_sync(&event) => {
              if let Err(error) = recover_rules(state).await {
                log::warn!("event::loop: DNS recovery failed: {error}");
                break;
              }
            }
            Ok(_) => {}
            Err(error) => {
              log::warn!("event::loop: {error}");
              break;
            }
          }
        }
      }
    }
    log::warn!("event::loop: retrying in {} seconds", RETRY_DELAY.as_secs());
    ntex::time::sleep(RETRY_DELAY).await;
  }
}

/// Register the resource kind and recover declarations on startup/reconnect.
pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      ensure_loop(&state).await;
      rt::Arbiter::current().stop();
    });
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_completed_generic_process_sync_triggers_recovery() {
    let mut event: Event = serde_json::from_value(serde_json::json!({
      "Key": "00000000-0000-0000-0000-000000000000",
      "CreatedAt": "2026-01-01T00:00:00",
      "ExpiresAt": "2026-01-01T00:00:00",
      "ReportingNode": "node",
      "ReportingController": "nanocl.io/core",
      "Kind": "normal",
      "Action": NativeEventAction::Update.to_string(),
      "Reason": "state_sync",
      "Actor": {"Kind": EventActorKind::Process}
    }))
    .unwrap();
    assert!(is_state_sync(&event));
    event.kind = EventKind::Error;
    assert!(!is_state_sync(&event));
    event.kind = EventKind::Normal;
    event.actor.as_mut().unwrap().key = Some("container-id".into());
    assert!(!is_state_sync(&event));
    event.actor.as_mut().unwrap().key = None;
    event.reason = "another_update".into();
    assert!(!is_state_sync(&event));
    event.reason = "state_sync".into();
    event.action = NativeEventAction::Start.to_string();
    assert!(!is_state_sync(&event));
  }
}
