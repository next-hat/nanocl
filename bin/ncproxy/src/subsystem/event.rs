use std::{future::Future, str::FromStr, sync::Arc, time::Duration};

use futures::{
  Stream, StreamExt,
  future::{Either, LocalBoxFuture, pending, select},
  stream::FuturesUnordered,
};
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

const RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RECOVERY_RETRY_DELAY: Duration = Duration::from_secs(30);

fn cargo_has_committed_start(
  cargo: &nanocld_client::stubs::cargo::CargoInspect,
) -> bool {
  cargo.status.actual == nanocld_client::stubs::system::ObjPsStatusKind::Start
}

fn cargo_event_refreshes_route(
  cargo: &nanocld_client::stubs::cargo::CargoInspect,
  action: &NativeEventAction,
) -> bool {
  if !cargo_has_committed_start(cargo) {
    return false;
  }
  // Preserve nightly's single handoff: healthless/NONE Cargoes refresh on
  // Start, while a health-gated Cargo refreshes on the following Healthy.
  // Nanocld commits aggregate health and actual state before either event.
  let healthchecked = cargo.status.health
    == nanocld_client::stubs::system::ObjPsHealthStatusKind::Healthy;
  match action {
    NativeEventAction::Start => !healthchecked,
    NativeEventAction::Healthy => healthchecked,
    _ => false,
  }
}

async fn cargo_event_should_refresh_route(
  name: &str,
  namespace: &str,
  action: &NativeEventAction,
  state: &SystemStateRef,
) -> IoResult<bool> {
  let key =
    nanocld_client::stubs::resource_key::ResourceKey::new(name, namespace)
      .map_err(|err| IoError::invalid_input("Cargo key", &err.to_string()))?;
  let cargo = state.client.inspect_cargo(key.as_str()).await?;
  Ok(cargo_event_refreshes_route(&cargo, action))
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
        utils::resource::refresh_rule(&resource.name, &rule, state).await
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
        utils::resource::refresh_rule(&resource.name, &rule, state).await
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
    (EventActorKind::Cargo, action @ NativeEventAction::Start)
    | (EventActorKind::Cargo, action @ NativeEventAction::Healthy) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      if cargo_event_should_refresh_route(&name, &namespace, &action, state)
        .await?
      {
        update_cargo_rule(&name, &namespace, state).await?;
        let _ = state.event_emitter.emit_reload().await;
      }
      Ok(())
    }
    (EventActorKind::Cargo, NativeEventAction::Unhealthy) => {
      let (name, namespace) = get_workload_attributes(&actor.attributes)?;
      // Recompute the serving replica set. A failed candidate can leave an
      // older ready generation routable, while an actually unhealthy replica
      // must disappear from the generated upstream.
      update_cargo_rule(&name, &namespace, state).await?;
      let _ = state.event_emitter.emit_reload().await;
      Ok(())
    }
    (EventActorKind::Cargo, NativeEventAction::Update) => {
      // Start/Healthy already performed the route handoff. Update is the
      // terminal rollout-completion signal for command and API waiters, so it
      // must not trigger a second reload.
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

/// Keep recovery and its retry timer owned by the subscribed stream. Ordinary
/// events neither cancel the pending retry nor wait for recovery to succeed.
/// Await each handler and recovery serially: event-driven route refreshes must
/// finish before another recovery resolves or commits its configuration.
async fn consume_events<S, E, H, HF, R, RF>(
  mut stream: S,
  mut handle: H,
  mut recover: R,
  retry_delay: Duration,
  max_retry_delay: Duration,
) where
  S: Stream<Item = Result<Event, E>> + Unpin,
  E: std::fmt::Display,
  H: FnMut(Event) -> HF,
  HF: Future<Output = IoResult<()>>,
  R: FnMut() -> RF,
  RF: Future<Output = IoResult<()>>,
{
  let mut retry: LocalBoxFuture<'static, ()> = Box::pin(async {});
  let mut next_delay = retry_delay;
  loop {
    // Poll the retained timer first so a busy event stream cannot postpone it.
    match select(&mut retry, stream.next()).await {
      Either::Left(_) => {}
      Either::Right((Some(Ok(event)), _))
        if event.is_process_inventory_sync() => {}
      Either::Right((Some(Ok(event)), _)) => {
        if let Err(err) = handle(event).await {
          log::warn!("event::loop: {err}");
        }
        continue;
      }
      Either::Right((Some(Err(err)), _)) => {
        log::warn!("event::loop: {err}");
        break;
      }
      Either::Right((None, _)) => break,
    }
    match recover().await {
      Ok(()) => {
        retry = Box::pin(pending());
        next_delay = retry_delay;
      }
      Err(err) => {
        log::warn!(
          "event::loop: reconciliation failed: {err}; retrying in {} seconds",
          next_delay.as_secs()
        );
        retry = Box::pin(ntex::time::sleep(next_delay));
        next_delay = next_delay.saturating_mul(2).min(max_retry_delay);
      }
    }
  }
}

async fn r#loop(state: &SystemStateRef) {
  loop {
    log::info!("event::loop: subscribing to nanocld events");
    match state.client.watch_events(None).await {
      Err(err) => {
        log::warn!("event::loop: {err}");
      }
      Ok(stream) => {
        if let Err(err) = ensure_self_config(&state.client).await {
          log::warn!("event::loop: {err}");
          ntex::time::sleep(RETRY_DELAY).await;
          continue;
        }
        // Initialization owns the base config; reconnecting must not replace
        // its health generation without an nginx reload.
        log::info!("event::loop: subscribed to nanocld events");
        consume_events(
          stream,
          |event| async move { on_event(&event, state).await },
          || utils::resource::rebuild_all_rules(state),
          RETRY_DELAY,
          MAX_RECOVERY_RETRY_DELAY,
        )
        .await;
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
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::{
    cargo::CargoInspect,
    system::{EventKind, ObjPsHealthStatusKind, ObjPsStatusKind},
  };

  use super::*;

  fn state_sync_event() -> Event {
    serde_json::from_value(serde_json::json!({
      "Key": "00000000-0000-0000-0000-000000000000",
      "CreatedAt": "2026-01-01T00:00:00",
      "ExpiresAt": "2026-01-01T00:00:00",
      "ReportingNode": "node",
      "ReportingController": "nanocl.io/core",
      "Kind": "normal",
      "Action": "update",
      "Reason": "state_sync",
      "Actor": {"Kind": EventActorKind::Process}
    }))
    .unwrap()
  }

  #[test]
  fn only_completed_inventory_events_trigger_recovery() {
    let event = state_sync_event();
    assert!(event.is_process_inventory_sync());
    let mut changed = event.clone();
    changed.kind = EventKind::Error;
    assert!(!changed.is_process_inventory_sync());
    changed = event.clone();
    changed.action = NativeEventAction::Healthy.to_string();
    assert!(!changed.is_process_inventory_sync());
    changed = event.clone();
    changed.reason = "health_status".into();
    assert!(!changed.is_process_inventory_sync());
    changed = event.clone();
    changed.actor.as_mut().unwrap().key = Some("container-id".into());
    assert!(!changed.is_process_inventory_sync());
    changed = event.clone();
    changed.actor.as_mut().unwrap().kind = EventActorKind::Cargo;
    assert!(!changed.is_process_inventory_sync());
    changed.actor = None;
    assert!(!changed.is_process_inventory_sync());
  }

  #[ntex::test]
  async fn failed_recovery_keeps_consuming_workload_and_secret_events() {
    let mut events = vec![state_sync_event()];
    for kind in [
      EventActorKind::Cargo,
      EventActorKind::Vm,
      EventActorKind::Secret,
    ] {
      let mut event = state_sync_event();
      event.actor.as_mut().unwrap().kind = kind;
      event.reason = "update".into();
      events.push(event);
    }
    let handled = std::cell::Cell::new(0);
    let recoveries = std::cell::Cell::new(0);
    consume_events(
      futures::stream::iter(events.into_iter().map(Ok::<_, IoError>)),
      |_| {
        handled.set(handled.get() + 1);
        futures::future::ready(Ok(()))
      },
      || {
        recoveries.set(recoveries.get() + 1);
        futures::future::ready(Err(IoError::other("Recovery", "unavailable")))
      },
      Duration::from_secs(3600),
      Duration::from_secs(3600),
    )
    .await;
    // Both startup and inventory-triggered recovery fail, but all ordinary
    // events are consumed before the pending retry; EOF cancels that timer.
    assert_eq!(recoveries.get(), 2);
    assert_eq!(handled.get(), 3);
  }

  #[ntex::test]
  async fn recovery_timer_retries_without_another_stream_event() {
    let (release, released) = futures::channel::oneshot::channel();
    let release = std::cell::RefCell::new(Some(release));
    let recoveries = std::cell::Cell::new(0);
    let handled = std::cell::Cell::new(0);
    let stream = Box::pin(futures::stream::once(async {
      released.await.unwrap();
      let mut event = state_sync_event();
      event.actor.as_mut().unwrap().kind = EventActorKind::Secret;
      Ok::<_, IoError>(event)
    }));
    ntex::time::timeout(
      Duration::from_secs(1),
      consume_events(
        stream,
        |_| {
          handled.set(handled.get() + 1);
          futures::future::ready(Ok(()))
        },
        || {
          recoveries.set(recoveries.get() + 1);
          futures::future::ready(if recoveries.get() == 1 {
            Err(IoError::other("Recovery", "temporarily unavailable"))
          } else {
            release.borrow_mut().take().unwrap().send(()).unwrap();
            Ok(())
          })
        },
        Duration::from_millis(1),
        Duration::from_millis(4),
      ),
    )
    .await
    .unwrap();
    assert_eq!(recoveries.get(), 2);
    assert_eq!(handled.get(), 1);
  }

  #[ntex::test]
  async fn yielding_event_handlers_and_recovery_never_overlap() {
    async fn yield_once() {
      let mut yielded = false;
      futures::future::poll_fn(move |cx| {
        if yielded {
          std::task::Poll::Ready(())
        } else {
          yielded = true;
          cx.waker().wake_by_ref();
          std::task::Poll::Pending
        }
      })
      .await;
    }

    let mut workload = state_sync_event();
    workload.actor.as_mut().unwrap().kind = EventActorKind::Cargo;
    let events = [workload.clone(), state_sync_event(), workload];
    let active = std::cell::Cell::new(None);
    let order = std::cell::RefCell::new(Vec::new());
    let consume = consume_events(
      futures::stream::iter(events.into_iter().map(Ok::<_, IoError>)),
      |_| async {
        assert_eq!(active.replace(Some("handler")), None);
        order.borrow_mut().push("handler started");
        yield_once().await;
        assert_eq!(active.take(), Some("handler"));
        order.borrow_mut().push("handler completed");
        Ok(())
      },
      || async {
        assert_eq!(active.replace(Some("recovery")), None);
        order.borrow_mut().push("recovery started");
        yield_once().await;
        assert_eq!(active.take(), Some("recovery"));
        order.borrow_mut().push("recovery completed");
        Ok(())
      },
      Duration::from_secs(1),
      Duration::from_secs(30),
    );
    futures::pin_mut!(consume);
    // Every operation yields while the remaining stream events are ready.
    // None may start another operation before its current one completes.
    for _ in 0..4 {
      assert!(futures::poll!(consume.as_mut()).is_pending());
    }
    assert!(futures::poll!(consume.as_mut()).is_ready());
    assert_eq!(active.get(), None);
    assert_eq!(
      *order.borrow(),
      [
        "recovery started",
        "recovery completed",
        "handler started",
        "handler completed",
        "recovery started",
        "recovery completed",
        "handler started",
        "handler completed",
      ]
    );
  }

  fn cargo(actual: ObjPsStatusKind) -> CargoInspect {
    CargoInspect {
      namespace_name: "global".to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      instance_total: 0,
      instance_running: 0,
      status: nanocld_client::stubs::system::ObjPsStatus {
        actual,
        ..Default::default()
      },
      spec: nanocld_client::stubs::cargo_spec::CargoSpecRevision {
        name: "api".to_owned(),
        cargo_key: "global.api".to_owned(),
        ..Default::default()
      },
      instances: Vec::new(),
    }
  }

  #[test]
  fn only_committed_start_is_ready_for_route_refresh() {
    assert!(cargo_has_committed_start(&cargo(ObjPsStatusKind::Start)));
    for status in [
      ObjPsStatusKind::Starting,
      ObjPsStatusKind::Updating,
      ObjPsStatusKind::Fail,
      ObjPsStatusKind::Stop,
    ] {
      assert!(!cargo_has_committed_start(&cargo(status)));
    }
  }

  #[test]
  fn cargo_promotion_refreshes_proxy_once_for_its_readiness_kind() {
    // Healthless and explicitly disabled healthchecks both retain aggregate
    // health Unknown and hand off on Start.
    let healthless = cargo(ObjPsStatusKind::Start);
    assert!(cargo_event_refreshes_route(
      &healthless,
      &NativeEventAction::Start
    ));
    assert!(!cargo_event_refreshes_route(
      &healthless,
      &NativeEventAction::Healthy
    ));
    assert!(!cargo_event_refreshes_route(
      &healthless,
      &NativeEventAction::Update
    ));

    let mut healthchecked = cargo(ObjPsStatusKind::Start);
    healthchecked.status.health = ObjPsHealthStatusKind::Healthy;
    assert!(!cargo_event_refreshes_route(
      &healthchecked,
      &NativeEventAction::Start
    ));
    assert!(cargo_event_refreshes_route(
      &healthchecked,
      &NativeEventAction::Healthy
    ));
    assert!(!cargo_event_refreshes_route(
      &healthchecked,
      &NativeEventAction::Update
    ));

    healthchecked.status.actual = ObjPsStatusKind::Updating;
    assert!(!cargo_event_refreshes_route(
      &healthchecked,
      &NativeEventAction::Healthy
    ));
  }

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
}
