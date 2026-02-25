use std::{str::FromStr, sync::Arc, time::{Duration, Instant}};

use futures::{StreamExt, stream::FuturesUnordered};
use ntex::rt;

use nanocl_error::io::{IoError, IoResult};

use nanocl_utils::versioning;
use nanocld_client::{
  NanocldClient,
  stubs::{
    resource::ResourcePartial,
    resource_kind::{ResourceKindPartial, ResourceKindSpec},
    system::Event,
    system::{EventActorKind, NativeEventAction},
  },
};

use crate::{models::SystemStateRef, utils, vars};

fn has_healthcheck(cargo: &nanocld_client::stubs::cargo::CargoInspect) -> bool {
  if cargo.spec.container.healthcheck.is_some() {
    return true;
  }
  cargo.instances.iter().any(|instance| {
    serde_json::to_value(&instance.data)
      .map(|value| {
        value.pointer("/Config/Healthcheck").is_some()
          || value.pointer("/State/Health").is_some()
      })
      .unwrap_or_default()
  })
}

fn count_healthy_instances(
  cargo: &nanocld_client::stubs::cargo::CargoInspect,
) -> usize {
  cargo
    .instances
    .iter()
    .filter(|instance| {
      serde_json::to_value(&instance.data)
        .ok()
        .and_then(|value| {
          value
            .pointer("/State/Health/Status")
            .and_then(|status| status.as_str())
            .map(|status| status == "healthy")
        })
        .unwrap_or_default()
    })
    .count()
}

async fn wait_cargo_health_quorum(
  name: &str,
  namespace: &str,
  state: &SystemStateRef,
) -> IoResult<()> {
  let timeout = Instant::now() + Duration::from_secs(120);
  loop {
    let cargo = state.client.inspect_cargo(name, Some(namespace)).await?;
    if !has_healthcheck(&cargo) {
      return Ok(());
    }
    if cargo.instances.is_empty() {
      if Instant::now() >= timeout {
        return Err(IoError::interrupted(
          "ProxyHealthTimeout",
          "Timeout waiting cargo instances to be available",
        ));
      }
      ntex::time::sleep(Duration::from_secs(1)).await;
      continue;
    }
    let needed = (cargo.instances.len() / 2) + 1;
    let healthy = count_healthy_instances(&cargo);
    if healthy >= needed {
      return Ok(());
    }
    if Instant::now() >= timeout {
      return Err(IoError::interrupted(
        "ProxyHealthTimeout",
        &format!(
          "Timeout waiting healthy quorum {healthy}/{needed} (total: {})",
          cargo.instances.len()
        ),
      ));
    }
    ntex::time::sleep(Duration::from_secs(1)).await;
  }
}

/// Get cargo attributes from nanocld event
fn get_cargo_attributes(
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
    (EventActorKind::Cargo, NativeEventAction::Start)
    | (EventActorKind::Cargo, NativeEventAction::Update) => {
      let (name, namespace) = get_cargo_attributes(&actor.attributes)?;
      wait_cargo_health_quorum(&name, &namespace, state).await?;
      update_cargo_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    (EventActorKind::Cargo, NativeEventAction::Stop)
    | (EventActorKind::Cargo, NativeEventAction::Destroy) => {
      let (name, namespace) = get_cargo_attributes(&actor.attributes)?;
      delete_cargo_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
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

async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
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
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(state: &SystemStateRef) {
  let state = Arc::clone(state);
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      r#loop(&state).await;
      rt::Arbiter::current().stop();
    });
  });
}
