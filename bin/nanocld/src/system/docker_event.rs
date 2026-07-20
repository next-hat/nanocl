use futures_util::StreamExt;
use ntex::rt;

use nanocl_error::io::{FromIo, IoError, IoResult};

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
    CargoDb, ObjPsStatusDb, ProcessDb, ProcessUpdateDb, SystemState, VmDb,
  },
  repositories::generic::*,
  utils, vars,
};

fn truncate_health_output(output: &str) -> String {
  const LIMIT: usize = 180;
  let mut compact = output
    .chars()
    .map(|character| {
      if character == '\n' || character == '\r' {
        ' '
      } else {
        character
      }
    })
    .collect::<String>();
  compact = compact.trim().to_owned();
  if compact.chars().count() <= LIMIT {
    return compact;
  }
  let truncated = compact.chars().take(LIMIT).collect::<String>();
  format!("{truncated}...")
}

fn is_runtime_cargo_instance(process_name: &str) -> bool {
  !process_name.starts_with("tmp-") && !process_name.starts_with("init-")
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

async fn unhealthy_instances_reason(
  instances: &[nanocl_stubs::process::Process],
  state: &SystemState,
) -> IoResult<Option<String>> {
  let mut reasons: Vec<String> = Vec::new();
  for process in instances {
    let inspected = match state
      .inner
      .docker_api
      .inspect_container(&process.key, None::<InspectContainerOptions>)
      .await
    {
      Ok(inspected) => inspected,
      Err(err) => {
        log::debug!("skip unhealthy reason for {}: {err}", process.name);
        continue;
      }
    };
    let Some(health) = inspected
      .state
      .as_ref()
      .and_then(|container_state| container_state.health.as_ref())
    else {
      continue;
    };
    let status = health
      .status
      .as_ref()
      .map(|status| status.to_string())
      .unwrap_or_else(|| "unknown".to_owned());
    if status != "unhealthy" {
      continue;
    }
    let failing_streak = health.failing_streak.unwrap_or_default();
    let (exit_code, output) = health
      .log
      .as_ref()
      .and_then(|logs| logs.last())
      .map(|entry| {
        (
          entry.exit_code.unwrap_or_default(),
          entry
            .output
            .as_ref()
            .map(|output| truncate_health_output(output))
            .unwrap_or_default(),
        )
      })
      .unwrap_or_default();
    reasons.push(format!(
      "{} status={status} failing_streak={failing_streak} exit_code={exit_code} output='{}'",
      process.name,
      if output.is_empty() {
        "no probe output".to_owned()
      } else {
        output
      }
    ));
  }
  if reasons.is_empty() {
    return Ok(None);
  }
  Ok(Some(reasons.join("; ")))
}

async fn count_health_states(
  instances: &[nanocl_stubs::process::Process],
  state: &SystemState,
) -> IoResult<(usize, usize, usize, usize)> {
  let mut healthy = 0usize;
  let mut unhealthy = 0usize;
  let mut starting = 0usize;
  let mut unknown = 0usize;
  for process in instances {
    let inspected = state
      .inner
      .docker_api
      .inspect_container(&process.key, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
    let status = inspected
      .state
      .as_ref()
      .and_then(|container_state| container_state.health.as_ref())
      .and_then(|health| health.status.as_ref())
      .map(|status| status.to_string())
      .unwrap_or_else(|| "unknown".to_owned());
    match status.as_str() {
      "healthy" => healthy += 1,
      "unhealthy" => unhealthy += 1,
      "starting" => starting += 1,
      _ => unknown += 1,
    }
  }
  Ok((healthy, unhealthy, starting, unknown))
}

async fn handle_cargo_health_event(
  kind_key: &str,
  action: &str,
  state: &SystemState,
) -> IoResult<()> {
  let cargo =
    CargoDb::transform_read_by_pk(kind_key, &state.inner.pool).await?;
  let instances =
    ProcessDb::read_by_kind_key(kind_key, None, &state.inner.pool).await?;
  let runtime_instances = instances
    .iter()
    .filter(|process| is_runtime_cargo_instance(&process.name))
    .cloned()
    .collect::<Vec<_>>();
  if runtime_instances.is_empty() {
    return Ok(());
  }
  if !utils::container::cargo::has_container_healthcheck(
    &cargo,
    &runtime_instances,
  ) {
    return Ok(());
  }
  let status = ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;

  if action == "health_status: unhealthy" {
    let reason = unhealthy_instances_reason(&runtime_instances, state)
      .await?
      .unwrap_or_else(|| "Cargo healthcheck reported unhealthy".to_owned());
    let should_emit_rollout_error = status.actual
      == ObjPsStatusKind::Updating.to_string()
      || status.actual == ObjPsStatusKind::Starting.to_string();
    let health_changed = ObjPsStatusDb::update_health_status(
      kind_key,
      &ObjPsHealthStatusKind::Unhealthy,
      &state.inner.pool,
    )
    .await?;
    if !health_changed && !should_emit_rollout_error {
      return Ok(());
    }
    let old_instances = instances
      .iter()
      .filter(|process| process.name.starts_with("tmp-"))
      .cloned()
      .collect::<Vec<_>>();
    if should_emit_rollout_error {
      if !old_instances.is_empty() {
        let new_instance_keys = runtime_instances
          .iter()
          .map(|process| process.key.clone())
          .collect::<Vec<_>>();
        if !new_instance_keys.is_empty() {
          let _ = utils::container::process::delete_instances(
            &new_instance_keys,
            state,
          )
          .await;
        }
      }
      for process in old_instances {
        let name = process.name.trim_start_matches("tmp-").to_owned();
        let _ = state
          .inner
          .docker_api
          .rename_container(
            &process.key,
            bollard_next::container::RenameContainerOptions { name: &name },
          )
          .await;
      }
    }
    ObjPsStatusDb::update_actual_status(
      kind_key,
      &ObjPsStatusKind::Fail,
      &state.inner.pool,
    )
    .await?;
    if should_emit_rollout_error {
      state.emit_error_native_action(
        &cargo,
        NativeEventAction::Start,
        Some(format!(
          "CargoStart: Cargo {} became unhealthy: {reason}",
          cargo.spec.cargo_key,
        )),
      );
    }
    if health_changed {
      state
        .emit_normal_native_action_sync(&cargo, NativeEventAction::Unhealthy)
        .await;
    }
    return Ok(());
  }

  if action == "health_status: healthy" {
    let (healthy, unhealthy, starting, unknown) =
      count_health_states(&runtime_instances, state).await?;
    if unhealthy > 0 || starting > 0 || unknown > 0 {
      log::debug!(
        "cargo health not ready {}: healthy={healthy} unhealthy={unhealthy} starting={starting} unknown={unknown}",
        cargo.spec.cargo_key
      );
      return Ok(());
    }
    if healthy != runtime_instances.len() {
      return Ok(());
    }

    let old_instance_keys = instances
      .iter()
      .filter(|process| process.name.starts_with("tmp-"))
      .map(|process| process.key.clone())
      .collect::<Vec<_>>();
    if !old_instance_keys.is_empty() {
      utils::container::process::delete_instances(&old_instance_keys, state)
        .await
        .map_err(|err| {
          IoError::interrupted(
            "CargoHealthCleanup",
            &format!("Unable to delete old cargo instances: {err}"),
          )
        })?;
    }

    let health_changed = ObjPsStatusDb::update_health_status(
      kind_key,
      &ObjPsHealthStatusKind::Healthy,
      &state.inner.pool,
    )
    .await?;

    let status = ObjPsStatusDb::read_by_pk(kind_key, &state.inner.pool).await?;
    if status.actual != ObjPsStatusKind::Start.to_string() {
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
  }
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
  let is_not_init_cargo_instance = attributes
    .get("io.nanocl.not-init-c")
    .map(|value| value == "true")
    .unwrap_or_default();
  let action = action.as_str();
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
      let actual_status =
        ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
      match (&kind, &actual_status.actual) {
        (EventActorKind::Cargo, status)
          if status != &ObjPsStatusKind::Start.to_string() =>
        {
          let cargo =
            CargoDb::transform_read_by_pk(&kind_key, &state.inner.pool).await?;
          let instances =
            ProcessDb::read_by_kind_key(&kind_key, None, &state.inner.pool)
              .await?;
          let runtime_instances = instances
            .iter()
            .filter(|process| is_runtime_cargo_instance(&process.name))
            .cloned()
            .collect::<Vec<_>>();
          let has_hc = utils::container::cargo::has_container_healthcheck(
            &cargo,
            &runtime_instances,
          )
            || utils::container::process::container_has_healthcheck(&id, state)
              .await?;
          if !has_hc {
            ObjPsStatusDb::update_actual_status(
              &kind_key,
              &ObjPsStatusKind::Start,
              &state.inner.pool,
            )
            .await?;
          }
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
      event.action = NativeEventAction::Update.to_string();
      if kind == EventActorKind::Cargo
        && is_not_init_cargo_instance
        && !name.starts_with("tmp-")
        && !name.starts_with("init-")
        && let Err(err) =
          handle_cargo_health_event(&kind_key, action, state).await
      {
        log::warn!("event::docker health_status: {err}");
      }
    }
    "die" => {
      if !name.starts_with("tmp-") && !name.starts_with("init-") {
        let actual_status =
          ObjPsStatusDb::read_by_pk(&kind_key, &state.inner.pool).await?;
        log::debug!(
          "Event status wanted/prev_actual {}/{}",
          actual_status.wanted,
          actual_status.prev_actual
        );
        match (&kind, &actual_status.wanted, &actual_status.prev_actual) {
          (EventActorKind::Cargo, wanted, prev_actual)
            if wanted != &ObjPsStatusKind::Stop.to_string()
              && wanted != &ObjPsStatusKind::Destroy.to_string()
              && prev_actual != &ObjPsStatusKind::Updating.to_string() =>
          {
            let cargo =
              CargoDb::transform_read_by_pk(&kind_key, &state.inner.pool)
                .await?;
            let instances =
              ProcessDb::read_by_kind_key(&kind_key, None, &state.inner.pool)
                .await?;
            let runtime_instances = instances
              .iter()
              .filter(|process| is_runtime_cargo_instance(&process.name))
              .cloned()
              .collect::<Vec<_>>();
            let has_hc = utils::container::cargo::has_container_healthcheck(
              &cargo,
              &runtime_instances,
            );
            if !has_hc {
              log::debug!("Set cargo status to fail");
              ObjPsStatusDb::update_actual_status(
                &kind_key,
                &ObjPsStatusKind::Fail,
                &state.inner.pool,
              )
              .await?;
              state.emit_error_native_action(
                &cargo,
                NativeEventAction::Die,
                Some(format!("process {name} die but should be alive")),
              );
            }
          }
          (EventActorKind::Vm, wanted, prev_actual)
            if wanted != &ObjPsStatusKind::Stop.to_string()
              && wanted != &ObjPsStatusKind::Destroy.to_string()
              && prev_actual != &ObjPsStatusKind::Updating.to_string() =>
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
  let instance = state
    .inner
    .docker_api
    .inspect_container(&id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let data = serde_json::to_value(instance)
    .map_err(|err| err.map_err_context(|| "Docker event"))?;
  let new_instance = ProcessUpdateDb {
    updated_at: Some(chrono::Utc::now().naive_utc()),
    data: Some(data),
    name: Some(name),
    ..Default::default()
  };
  ProcessDb::update_pk(&id, new_instance, &state.inner.pool).await?;
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
}
