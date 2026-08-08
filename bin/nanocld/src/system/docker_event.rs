use std::collections::HashMap;

use futures_util::StreamExt;
use ntex::rt;

use nanocl_error::io::{FromIo, IoResult};

use bollard_next::{
  container::InspectContainerOptions,
  service::{EventMessage, EventMessageTypeEnum},
  system::EventsOptions,
};
use nanocl_stubs::system::{
  EventActor, EventActorKind, EventKind, EventPartial, NativeEventAction,
  ObjPsHealthStatusKind, ObjPsStatusKind,
};

use crate::{
  models::{
    CargoDb, CargoReplicaDb, ObjPsStatusDb, ProcessDb, ProcessUpdateDb,
    SystemState, VmDb,
  },
  repositories::generic::*,
  utils, vars,
};

const LABEL_CARGO_ROLE: &str = "io.nanocl.cargo.role";
const LABEL_CARGO_ESSENTIAL: &str = "io.nanocl.cargo.essential";
const LABEL_LEGACY_APPLICATION: &str = "io.nanocl.not-init-c";

fn labels_describe_active_cargo_application(
  process_name: &str,
  labels: Option<&HashMap<String, String>>,
) -> bool {
  if process_name.starts_with("tmp-")
    || process_name.starts_with("candidate-")
    || process_name.starts_with("init-")
  {
    return false;
  }
  let Some(labels) = labels else {
    return false;
  };
  match labels.get(LABEL_CARGO_ROLE).map(String::as_str) {
    Some("app") => labels
      .get(LABEL_CARGO_ESSENTIAL)
      .is_some_and(|value| value == "true"),
    Some(_) => false,
    None => labels
      .get(LABEL_LEGACY_APPLICATION)
      .is_some_and(|value| value == "true"),
  }
}

fn labels_describe_active_cargo_runtime(
  process_name: &str,
  labels: Option<&HashMap<String, String>>,
) -> bool {
  if process_name.starts_with("tmp-")
    || process_name.starts_with("candidate-")
    || process_name.starts_with("init-")
  {
    return false;
  }
  let Some(labels) = labels else {
    return false;
  };
  match labels.get(LABEL_CARGO_ROLE).map(String::as_str) {
    Some("sandbox") => true,
    Some("app") => labels
      .get(LABEL_CARGO_ESSENTIAL)
      .is_some_and(|value| value == "true"),
    Some(_) => false,
    None => labels
      .get(LABEL_LEGACY_APPLICATION)
      .is_some_and(|value| value == "true"),
  }
}

fn reconciliation_owns_cargo_health(actual: &str) -> bool {
  actual == ObjPsStatusKind::Updating.to_string()
    || actual == ObjPsStatusKind::Starting.to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NetworkEventOperation {
  Refresh,
  Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NetworkEvent {
  operation: NetworkEventOperation,
  docker_reference: String,
  name: Option<String>,
}

fn parse_network_event(event: &EventMessage) -> Option<NetworkEvent> {
  if event.typ != Some(EventMessageTypeEnum::NETWORK) {
    return None;
  }
  let operation = match event.action.as_deref()? {
    "create" | "connect" | "disconnect" | "update" => {
      NetworkEventOperation::Refresh
    }
    "destroy" | "remove" => NetworkEventOperation::Remove,
    _ => return None,
  };
  let actor = event.actor.as_ref()?;
  let name = actor
    .attributes
    .as_ref()
    .and_then(|attributes| attributes.get("name"))
    .filter(|name| !name.is_empty())
    .cloned();
  let docker_reference = actor
    .id
    .as_deref()
    .filter(|id| !id.is_empty())
    .map(str::to_owned)
    .or_else(|| name.clone())?;
  Some(NetworkEvent {
    operation,
    docker_reference,
    name,
  })
}

async fn exec_network_event(
  event: &EventMessage,
  state: &SystemState,
) -> IoResult<()> {
  let Some(event) = parse_network_event(event) else {
    return Ok(());
  };
  match event.operation {
    NetworkEventOperation::Refresh => {
      utils::container::network::refresh_network(
        &event.docker_reference,
        event.name.as_deref(),
        state,
      )
      .await?;
    }
    NetworkEventOperation::Remove => {
      let Some(name) = event.name else {
        log::warn!(
          "event::network: remove event {} has no network name",
          event.docker_reference
        );
        return Ok(());
      };
      // Docker may deliver an old destroy event after another workload has
      // recreated the same name. Refresh through the old ID first, then the
      // canonical name, so that a replacement snapshot is preserved.
      utils::container::network::refresh_network(
        &event.docker_reference,
        Some(&name),
        state,
      )
      .await?;
    }
  }
  Ok(())
}

async fn refresh_process_observation(
  id: &str,
  name: &str,
  state: &SystemState,
) -> IoResult<()> {
  let instance = state
    .inner
    .docker_api
    .inspect_container(id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  ProcessDb::update_pk(
    id,
    ProcessUpdateDb {
      updated_at: Some(chrono::Utc::now().naive_utc()),
      data: Some(data),
      name: Some(name.to_owned()),
      ..Default::default()
    },
    &state.inner.pool,
  )
  .await?;
  Ok(())
}

async fn reduce_cargo_runtime_readiness(
  kind_key: &str,
  trigger: &str,
  state: &SystemState,
) -> IoResult<bool> {
  let status = ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
  if status.wanted != ObjPsStatusKind::Start.to_string() {
    log::debug!(
      "ignore Cargo readiness trigger while wanted={}: key={kind_key} trigger={trigger}",
      status.wanted
    );
    return Ok(false);
  }
  if reconciliation_owns_cargo_health(&status.actual) {
    // Cargo reconciliation performs a synchronous, per-replica readiness
    // gate and owns promotion or rollback. Docker events can be delivered out
    // of order, so they must not mutate aggregate state or delete/restore a
    // generation while reconciliation is in progress.
    log::debug!(
      "ignore Cargo readiness trigger during {}: key={kind_key} trigger={trigger}",
      status.actual
    );
    return Ok(false);
  }
  let cargo =
    CargoDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
  let instances =
    ProcessDb::read_by_kind_key(kind_key, None, &state.inner.pool).await?;
  let desired_replica_keys =
    CargoReplicaDb::list_by_cargo(kind_key, &state.inner.pool)
      .await?
      .into_iter()
      .map(|replica| replica.key)
      .collect();
  let ready = utils::container::cargo::all_desired_replicas_ready(
    &cargo,
    &desired_replica_keys,
    &instances,
  );
  if !ready {
    let health_changed = ObjPsStatusDb::update_health_status(
      kind_key,
      &ObjPsHealthStatusKind::Unhealthy,
      &state.inner.pool,
    )
    .await?;
    let actual_changed = status.actual != ObjPsStatusKind::Fail.to_string();
    if actual_changed {
      ObjPsStatusDb::update_actual_status(
        kind_key,
        &ObjPsStatusKind::Fail,
        &state.inner.pool,
      )
      .await?;
    }
    if health_changed || actual_changed {
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Unhealthy)
        .await;
    }
    return Ok(false);
  }
  let health_changed = ObjPsStatusDb::update_health_status(
    kind_key,
    &ObjPsHealthStatusKind::Healthy,
    &state.inner.pool,
  )
  .await?;
  let actual_changed = status.actual != ObjPsStatusKind::Start.to_string();
  if actual_changed {
    ObjPsStatusDb::update_actual_status(
      kind_key,
      &ObjPsStatusKind::Start,
      &state.inner.pool,
    )
    .await?;
    state
      .emit_normal_native_action_sync(&cargo, NativeEventAction::Start)
      .await;
  }
  if health_changed {
    state
      .emit_normal_native_action_sync(&cargo, NativeEventAction::Healthy)
      .await;
  }
  Ok(true)
}

async fn handle_cargo_health_event(
  kind_key: &str,
  action: &str,
  state: &SystemState,
) -> IoResult<()> {
  let _ = reduce_cargo_runtime_readiness(kind_key, action, state).await?;
  Ok(())
}

/// Take actions when a docker event is received
async fn exec_docker(
  event: &EventMessage,
  state: &SystemState,
) -> IoResult<()> {
  let kind = event.typ.unwrap_or(EventMessageTypeEnum::EMPTY);
  match kind {
    EventMessageTypeEnum::NETWORK => {
      return exec_network_event(event, state).await;
    }
    EventMessageTypeEnum::CONTAINER => {}
    _ => return Ok(()),
  }
  let actor = event.actor.clone().unwrap_or_default();
  let attributes = actor.attributes.unwrap_or_default();
  if !attributes.contains_key("io.nanocl") {
    return Ok(());
  }
  let Some(kind) = attributes.get("io.nanocl.kind") else {
    return Ok(());
  };
  let (kind, kind_key) = match kind.as_str() {
    "cargo" => (
      EventActorKind::Cargo,
      attributes.get("io.nanocl.c").cloned(),
    ),
    "vm" => (EventActorKind::Vm, attributes.get("io.nanocl.v").cloned()),
    "job" => (EventActorKind::Job, attributes.get("io.nanocl.j").cloned()),
    _ => {
      return Ok(());
    }
  };
  let Some(kind_key) = kind_key else {
    return Ok(());
  };
  let action = event.action.clone().unwrap_or_default();
  let id = actor.id.unwrap_or_default();
  let name = attributes.get("name").cloned().unwrap_or_default();
  let is_active_cargo_application_event =
    labels_describe_active_cargo_application(&name, Some(&attributes));
  let is_active_cargo_runtime_event =
    labels_describe_active_cargo_runtime(&name, Some(&attributes));
  let action = action.as_str();
  let mut observation_refreshed = false;
  let mut event = EventPartial {
    reporting_controller: vars::CONTROLLER_NAME.to_owned(),
    reporting_node: state.inner.config.hostname.clone(),
    kind: EventKind::Normal,
    action: NativeEventAction::Destroy.to_string(),
    related: Some(EventActor {
      key: Some(kind_key.clone()),
      kind: kind.clone(),
      attributes: None,
    }),
    reason: "state_sync".to_owned(),
    note: Some(format!("process {name} {action}")),
    metadata: None,
    actor: Some(EventActor {
      key: Some(name.clone()),
      kind: EventActorKind::Process,
      attributes: Some(
        serde_json::to_value(attributes.clone())
          .map_err(|err| err.map_err_context(|| "Event attributes"))?,
      ),
    }),
  };
  match action {
    "start" => {
      if kind == EventActorKind::Cargo {
        refresh_process_observation(&id, &name, state).await?;
        observation_refreshed = true;
      }
      let actual_status =
        ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
      match (&kind, &actual_status.actual) {
        // Cargo start and rollout readiness are reduced synchronously by the
        // Cargo reconciler. A sandbox, init container, or the first app start
        // event must never promote the whole Cargo.
        (EventActorKind::Cargo, actual)
          if !reconciliation_owns_cargo_health(actual)
            && actual_status.wanted == ObjPsStatusKind::Start.to_string() =>
        {
          let _ =
            reduce_cargo_runtime_readiness(&kind_key, "container start", state)
              .await?;
        }
        (EventActorKind::Vm, status)
          if status != &ObjPsStatusKind::Start.to_string() =>
        {
          ObjPsStatusDb::update_actual_status(
            &kind_key,
            &ObjPsStatusKind::Start,
            &state.inner.pool,
          )
          .await?;
        }
        _ => {}
      }
    }
    action if action.starts_with("health_status:") => {
      refresh_process_observation(&id, &name, state).await?;
      observation_refreshed = true;
      event.action = NativeEventAction::Update.to_string();
      if kind == EventActorKind::Cargo
        && is_active_cargo_application_event
        && let Err(err) =
          handle_cargo_health_event(&kind_key, action, state).await
      {
        log::warn!("event::docker health_status: {err}");
      }
    }
    "die" => {
      refresh_process_observation(&id, &name, state).await?;
      observation_refreshed = true;
      if kind != EventActorKind::Cargo || is_active_cargo_runtime_event {
        let actual_status =
          ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
        log::debug!(
          "Event status wanted/actual {}/{}",
          actual_status.wanted,
          actual_status.actual
        );
        match (&kind, &actual_status.wanted, &actual_status.actual) {
          (EventActorKind::Cargo, wanted, actual)
            if wanted != &ObjPsStatusKind::Stop.to_string()
              && wanted != &ObjPsStatusKind::Destroy.to_string()
              && !reconciliation_owns_cargo_health(actual) =>
          {
            let cargo =
              CargoDb::transform_read_by_pk(&kind_key, &state.inner.pool)
                .await?;
            if !reduce_cargo_runtime_readiness(
              &kind_key,
              "container die",
              state,
            )
            .await?
            {
              state.emit_error_native_action(
                &cargo,
                NativeEventAction::Die,
                Some(format!("process {name} die but should be alive")),
              );
            }
          }
          (EventActorKind::Vm, wanted, _)
            if wanted != &ObjPsStatusKind::Stop.to_string()
              && wanted != &ObjPsStatusKind::Destroy.to_string()
              && actual_status.prev_actual
                != ObjPsStatusKind::Updating.to_string() =>
          {
            ObjPsStatusDb::update_actual_status(
              &kind_key,
              &ObjPsStatusKind::Fail,
              &state.inner.pool,
            )
            .await?;
            let vm =
              VmDb::transform_read_by_pk(&kind_key, &state.inner.pool).await?;
            state.emit_error_native_action(
              &vm,
              NativeEventAction::Die,
              Some(format!("process {name} die but should be alive")),
            );
          }
          _ => {}
        }
      }
      action.clone_into(&mut event.action);
    }
    "destroy" => {
      state.spawn_emit_event(event);
      let _ = ProcessDb::del_by_pk(&id, &state.inner.pool).await;
      return Ok(());
    }
    "create" => {
      event.action = NativeEventAction::Create.to_string();
      state.spawn_emit_event(event);
      return Ok(());
    }
    _ => {
      action.clone_into(&mut event.action);
    }
  }
  state.spawn_emit_event(event);
  if !observation_refreshed {
    refresh_process_observation(&id, &name, state).await?;
  }
  Ok(())
}

/// Create a new thread with his own loop to analyze events from docker
pub fn analyze(state: &SystemState) {
  const NETWORK_RECONCILE_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30);

  let reconcile_state = state.clone();
  rt::spawn(async move {
    loop {
      ntex::time::sleep(NETWORK_RECONCILE_INTERVAL).await;
      if let Err(err) =
        utils::container::network::reconcile_networks(&reconcile_state).await
      {
        log::warn!("event::network: periodic reconciliation failed: {err}");
      }
    }
  });

  let state = state.clone();
  rt::Arbiter::new().handle().spawn(async move {
    loop {
      let mut streams =
        state.inner.docker_api.events(None::<EventsOptions<String>>);
      log::info!("event::analyze_docker: stream connected");
      let reconnect_state = state.clone();
      rt::spawn(async move {
        if let Err(err) =
          utils::container::network::reconcile_networks(&reconnect_state).await
        {
          log::warn!("event::network: reconnect reconciliation failed: {err}");
        }
      });
      while let Some(event) = streams.next().await {
        match event {
          Ok(event) => {
            if let Err(err) = exec_docker(&event, &state).await {
              log::warn!("event::analyze_docker: {err}")
            }
          }
          Err(err) => {
            log::warn!("event::analyze_docker: {err}");
          }
        }
      }
      log::warn!("event::analyze_docker: disconnected trying to reconnect");
      ntex::time::sleep(std::time::Duration::from_secs(1)).await;
    }
  });
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use bollard_next::service::EventActor as DockerEventActor;

  use super::*;

  fn network_event(action: &str) -> EventMessage {
    EventMessage {
      typ: Some(EventMessageTypeEnum::NETWORK),
      action: Some(action.to_owned()),
      actor: Some(DockerEventActor {
        id: Some("network-id".to_owned()),
        attributes: Some(HashMap::from([
          ("name".to_owned(), "private-api".to_owned()),
          ("type".to_owned(), "bridge".to_owned()),
          ("container".to_owned(), "container-id".to_owned()),
        ])),
      }),
      ..Default::default()
    }
  }

  #[test]
  fn network_lifecycle_actions_are_classified() {
    for action in ["create", "connect", "disconnect", "update"] {
      let event = parse_network_event(&network_event(action)).unwrap();
      assert_eq!(event.operation, NetworkEventOperation::Refresh);
      assert_eq!(event.docker_reference, "network-id");
      assert_eq!(event.name.as_deref(), Some("private-api"));
    }
    for action in ["destroy", "remove"] {
      let event = parse_network_event(&network_event(action)).unwrap();
      assert_eq!(event.operation, NetworkEventOperation::Remove);
    }
  }

  #[test]
  fn container_id_is_not_used_as_the_network_reference() {
    let event = parse_network_event(&network_event("connect")).unwrap();
    assert_eq!(event.docker_reference, "network-id");
  }

  #[test]
  fn remove_event_keeps_old_id_and_canonical_name_for_reconciliation() {
    let event = parse_network_event(&network_event("destroy")).unwrap();
    assert_eq!(event.operation, NetworkEventOperation::Remove);
    assert_eq!(event.docker_reference, "network-id");
    assert_eq!(event.name.as_deref(), Some("private-api"));
  }

  #[test]
  fn unrelated_events_are_ignored() {
    let mut event = network_event("connect");
    event.typ = Some(EventMessageTypeEnum::CONTAINER);
    assert!(parse_network_event(&event).is_none());

    let event = network_event("prune");
    assert!(parse_network_event(&event).is_none());
  }

  #[test]
  fn network_name_is_a_fallback_when_actor_id_is_missing() {
    let mut event = network_event("create");
    event.actor.as_mut().unwrap().id = None;
    let event = parse_network_event(&event).unwrap();
    assert_eq!(event.docker_reference, "private-api");
  }

  #[test]
  fn only_active_application_labels_participate_in_cargo_health() {
    let app = HashMap::from([
      (LABEL_CARGO_ROLE.to_owned(), "app".to_owned()),
      (LABEL_CARGO_ESSENTIAL.to_owned(), "true".to_owned()),
    ]);
    let nonessential = HashMap::from([
      (LABEL_CARGO_ROLE.to_owned(), "app".to_owned()),
      (LABEL_CARGO_ESSENTIAL.to_owned(), "false".to_owned()),
    ]);
    let init =
      HashMap::from([(LABEL_CARGO_ROLE.to_owned(), "init".to_owned())]);
    let sandbox =
      HashMap::from([(LABEL_CARGO_ROLE.to_owned(), "sandbox".to_owned())]);
    let legacy =
      HashMap::from([(LABEL_LEGACY_APPLICATION.to_owned(), "true".to_owned())]);

    assert!(labels_describe_active_cargo_application(
      "demo-api.c",
      Some(&app)
    ));
    assert!(labels_describe_active_cargo_application(
      "demo-api.c",
      Some(&legacy)
    ));
    assert!(!labels_describe_active_cargo_application(
      "demo-sidecar.c",
      Some(&nonessential)
    ));
    assert!(!labels_describe_active_cargo_application(
      "tmp-demo-api.c",
      Some(&app)
    ));
    assert!(!labels_describe_active_cargo_application(
      "init-demo-db.c",
      Some(&init)
    ));
    assert!(!labels_describe_active_cargo_application(
      "sandbox-demo.c",
      Some(&sandbox)
    ));
    assert!(!labels_describe_active_cargo_application(
      "demo-api.c",
      None
    ));
  }

  #[test]
  fn reconciliation_owns_starting_and_updating_health_events() {
    assert!(reconciliation_owns_cargo_health(
      &ObjPsStatusKind::Starting.to_string()
    ));
    assert!(reconciliation_owns_cargo_health(
      &ObjPsStatusKind::Updating.to_string()
    ));
    assert!(!reconciliation_owns_cargo_health(
      &ObjPsStatusKind::Start.to_string()
    ));
    assert!(!reconciliation_owns_cargo_health(
      &ObjPsStatusKind::Fail.to_string()
    ));
  }
}
