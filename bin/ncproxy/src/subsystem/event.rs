use std::{str::FromStr, sync::Arc};

use futures::{StreamExt, stream::FuturesUnordered};
use ntex::rt;

use nanocl_error::io::{IoError, IoResult};

use nanocl_utils::versioning;
use nanocld_client::{
  NanocldClient,
  stubs::{
    proxy::ResourceProxyRule,
    resource::ResourcePartial,
    resource_kind::{ResourceKindPartial, ResourceKindSpec},
    system::Event,
    system::{EventActor, EventActorKind, NativeEventAction},
  },
};

use crate::{models::SystemStateRef, utils, vars};

const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
const RECONCILE_INTERVAL: std::time::Duration =
  std::time::Duration::from_secs(30);

fn is_proxy_resource(actor: &EventActor) -> bool {
  actor
    .attributes
    .as_ref()
    .and_then(|attributes| attributes.get("Kind"))
    .and_then(serde_json::Value::as_str)
    == Some(vars::RULE_KEY)
}

fn proxy_resource_name(actor: &EventActor) -> IoResult<&str> {
  actor
    .key
    .as_deref()
    .filter(|name| !name.is_empty())
    .ok_or_else(|| IoError::invalid_data("Resource event", "Missing key"))
}

fn proxy_resource_rule(actor: &EventActor) -> IoResult<ResourceProxyRule> {
  let data = actor
    .attributes
    .as_ref()
    .and_then(|attributes| attributes.get("Spec"))
    .ok_or_else(|| IoError::invalid_data("Resource event", "Missing Spec"))?;
  utils::resource::serialize(data)
}

fn has_healthcheck(cargo: &nanocld_client::stubs::cargo::CargoInspect) -> bool {
  if cargo.spec.container.healthcheck.is_some() {
    return true;
  }
  cargo.instances.iter().any(|instance| {
    instance
      .data
      .config
      .as_ref()
      .and_then(|config| config.healthcheck.as_ref())
      .is_some()
      || instance
        .data
        .state
        .as_ref()
        .and_then(|container_state| container_state.health.as_ref())
        .is_some()
  })
}

async fn should_wait_for_healthy_event(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<bool> {
  let cargo = state.client.inspect_cargo(name, Some(namespace)).await?;
  Ok(has_healthcheck(&cargo))
}

/// Get workload name and namespace attributes from a nanocld event.
fn get_workload_attributes(
  attributes: &Option<serde_json::Value>,
) -> IoResult<(String, String)> {
  let attributes = attributes.clone().unwrap_or_default();
  let name = attributes
    .get("Name")
    .map(|e| e.as_str().unwrap_or_default().to_owned())
    .ok_or_else(|| IoError::invalid_data("Attribute Name", "Missing value"))?;
  let namespace_name = attributes
    .get("Namespace")
    .map(|e| e.as_str().unwrap_or_default().to_owned())
    .ok_or_else(|| {
      IoError::invalid_data("Attribute Namespace", "Missing value")
    })?;
  Ok((name, namespace_name))
}

/// Update the nginx configuration when a cargo is started, patched
async fn update_cargo_rule(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<()> {
  let resources = utils::resource::list_by_cargo(
    name,
    Some(namespace.to_owned()),
    &state.client,
  )
  .await?;
  resources
    .into_iter()
    .map(|resource| async {
      let resource: ResourcePartial = resource.into();
      let rule = utils::resource::serialize(&resource.data)?;
      if let Err(err) =
        utils::nginx::add_rule(&resource.name, &rule, state).await
      {
        log::warn!("event::update_cargo_rule: {err}");
      }
      Ok::<_, IoError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, IoError>>()?;
  Ok(())
}

/// Update the nginx configuration when a cargo is stopped, deleted
async fn delete_cargo_rule(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<()> {
  let resources = utils::resource::list_by_cargo(
    name,
    Some(namespace.to_owned()),
    &state.client,
  )
  .await?;
  utils::resource::update_rules(&resources, state).await?;
  Ok(())
}

/// Update nginx rules targeting a VM after it starts or is recreated.
async fn update_vm_rule(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<()> {
  let resources = utils::resource::list_by_vm(
    name,
    Some(namespace.to_owned()),
    &state.client,
  )
  .await?;
  resources
    .into_iter()
    .map(|resource| async {
      let resource: ResourcePartial = resource.into();
      let rule = utils::resource::serialize(&resource.data)?;
      if let Err(err) =
        utils::nginx::add_rule(&resource.name, &rule, state).await
      {
        log::warn!("event::update_vm_rule: {err}");
      }
      Ok::<_, IoError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, IoError>>()?;
  Ok(())
}

/// Rebuild rules targeting a VM after its runtime has been destroyed.
async fn delete_vm_rule(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<()> {
  let resources = utils::resource::list_by_vm(
    name,
    Some(namespace.to_owned()),
    &state.client,
  )
  .await?;
  utils::resource::update_rules(&resources, state).await?;
  Ok(())
}

/// Analyze nanocld events and update nginx configuration
async fn on_event(event: &Event, state: &SystemStateRef) -> IoResult<()> {
  let action = NativeEventAction::from_str(&event.action)?;
  let kind = &event.kind;
  let Some(actor) = event.actor.clone() else {
    return Ok(());
  };
  let actor_kind = &actor.kind;
  log::trace!("event::on_event: {kind} {action} {actor_kind}");
  match (actor_kind, action) {
    (EventActorKind::Cargo, NativeEventAction::Healthy) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      update_cargo_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    (EventActorKind::Cargo, NativeEventAction::Start)
    | (EventActorKind::Cargo, NativeEventAction::Update) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      if !should_wait_for_healthy_event(&name, &namespace, state).await? {
        update_cargo_rule(&name, &namespace, state).await?;
        let _ = state.event_emitter.emit_reload().await;
      }
      Ok(())
    }
    (EventActorKind::Cargo, NativeEventAction::Stop)
    | (EventActorKind::Cargo, NativeEventAction::Destroy) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      delete_cargo_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    (EventActorKind::Vm, NativeEventAction::Start)
    | (EventActorKind::Vm, NativeEventAction::Update) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      update_vm_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    (EventActorKind::Vm, NativeEventAction::Stop)
    | (EventActorKind::Vm, NativeEventAction::Destroy) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      delete_vm_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    // This shouldn't be required as nanocld call the endpoint.
    // (EventActorKind::Resource, NativeEventAction::Create)
    // | (EventActorKind::Resource, NativeEventAction::Update) => {
    //   if !is_proxy_resource(&actor) {
    //     return Ok(());
    //   }
    //   let name = proxy_resource_name(&actor)?;
    //   let rule = proxy_resource_rule(&actor)?;
    //   utils::nginx::add_rule(name, &rule, state).await?;
    //   state.event_emitter.emit_reload().await;
    //   Ok(())
    // }
    // (EventActorKind::Resource, NativeEventAction::Destroy) => {
    //   if !is_proxy_resource(&actor) {
    //     return Ok(());
    //   }
    //   let name = proxy_resource_name(&actor)?;
    //   utils::nginx::del_rule(name, state).await?;
    //   state.event_emitter.emit_reload().await;
    //   Ok(())
    // }
    (EventActorKind::Secret, NativeEventAction::Create)
    | (EventActorKind::Secret, NativeEventAction::Update) => {
      let resources = utils::resource::list_by_secret(
        &actor.key.unwrap_or_default(),
        &state.client,
      )
      .await?;
      utils::resource::update_rules(&resources, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    _ => Ok(()),
  }
}

async fn reconcile_loop(state: &SystemStateRef) {
  loop {
    log::info!("event::reconcile: rebuilding committed proxy resources");
    if let Err(err) = utils::resource::rebuild_all_rules(state).await {
      log::warn!("event::reconcile: {err}");
    }
    ntex::time::sleep(RECONCILE_INTERVAL).await;
  }
}

pub(super) async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formatted_version = versioning::format_version(vars::VERSION);
  let resource_kind = ResourceKindPartial {
    name: "ncproxy.io/rule".to_owned(),
    version: format!("v{formatted_version}"),
    metadata: None,
    data: ResourceKindSpec {
      schema: None,
      url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
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

async fn r#loop(state: &SystemStateRef) {
  loop {
    log::info!("event::loop: subscribing to nanocld events");
    match state.client.watch_events(None).await {
      Err(err) => {
        log::warn!("event::loop: {err}");
      }
      Ok(mut stream) => {
        if let Err(err) = ensure_self_config(&state.client).await {
          log::warn!("event::loop: {err}");
          continue;
        }
        let _ = utils::nginx::ensure_conf(state).await;
        if let Err(err) = utils::resource::rebuild_all_rules(state).await {
          log::warn!("event::loop: initial reconciliation failed: {err}");
        }
        log::info!("event::loop: subscribed to nanocld events");
        while let Some(event) = stream.next().await {
          let event = match event {
            Err(err) => {
              log::warn!("event::loop: {err}");
              continue;
            }
            Ok(event) => event,
          };
          if let Err(err) = on_event(&event, state).await {
            log::warn!("event::loop: {err}");
          }
        }
      }
    }
    log::warn!("event::loop: retrying in 2 seconds");
    ntex::time::sleep(RETRY_DELAY).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(state: &SystemStateRef) {
  let event_state = Arc::clone(state);
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      r#loop(&event_state).await;
      rt::Arbiter::current().stop();
    });
  });
  // let reconcile_state = Arc::clone(state);
  // rt::Arbiter::new().handle().spawn(async move {
  //   rt::spawn(async move {
  //     reconcile_loop(&reconcile_state).await;
  //     rt::Arbiter::current().stop();
  //   });
  // });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn workload_attributes_are_shared_by_cargo_and_vm_events() {
    let attributes = Some(serde_json::json!({
      "Name": "database",
      "Namespace": "private",
    }));
    assert_eq!(
      get_workload_attributes(&attributes).unwrap(),
      ("database".to_owned(), "private".to_owned())
    );
  }

  #[test]
  fn committed_proxy_resource_events_expose_name_and_rule() {
    let actor = EventActor {
      key: Some("public-api".to_owned()),
      kind: EventActorKind::Resource,
      attributes: Some(serde_json::json!({
        "Kind": vars::RULE_KEY,
        "Version": "v0.15.0",
        "Spec": { "Rules": [] },
      })),
    };

    assert!(is_proxy_resource(&actor));
    assert_eq!(proxy_resource_name(&actor).unwrap(), "public-api");
    assert!(proxy_resource_rule(&actor).unwrap().rules.is_empty());
  }

  #[test]
  fn resource_events_for_other_controllers_are_ignored() {
    let actor = EventActor {
      key: Some("dns-rule".to_owned()),
      kind: EventActorKind::Resource,
      attributes: Some(serde_json::json!({
        "Kind": "ncdns.io/rule",
        "Spec": { "Rules": [] },
      })),
    };

    assert!(!is_proxy_resource(&actor));
  }
}
