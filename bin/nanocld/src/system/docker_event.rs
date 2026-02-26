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
  ObjPsStatusKind,
};

use crate::{
  models::{
    CargoDb, ObjPsStatusDb, ProcessDb, ProcessUpdateDb, SystemState, VmDb,
  },
  repositories::generic::*,
  utils, vars,
};

fn health_quorum(total: usize) -> usize {
  (total / 2) + 1
}

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

async fn unhealthy_instances_reason(
  instances: &[nanocl_stubs::process::Process],
  state: &SystemState,
) -> IoResult<Option<String>> {
  let mut reasons: Vec<String> = Vec::new();
  for process in instances {
    let inspected = state
      .inner
      .docker_api
      .inspect_container(&process.key, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
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

async fn has_healthcheck(
  cargo: &nanocl_stubs::cargo::Cargo,
  instances: &[nanocl_stubs::process::Process],
  state: &SystemState,
) -> IoResult<bool> {
  if cargo.spec.container.healthcheck.is_some() {
    return Ok(true);
  }
  for process in instances {
    let inspected = state
      .inner
      .docker_api
      .inspect_container(&process.key, None::<InspectContainerOptions>)
      .await
      .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
    let has_config_healthcheck = inspected
      .config
      .as_ref()
      .and_then(|config| config.healthcheck.as_ref())
      .is_some();
    let has_state_health = inspected
      .state
      .as_ref()
      .and_then(|container_state| container_state.health.as_ref())
      .is_some();
    if has_config_healthcheck || has_state_health {
      return Ok(true);
    }
  }
  Ok(false)
}

async fn container_id_has_healthcheck(
  container_id: &str,
  state: &SystemState,
) -> IoResult<bool> {
  if container_id.is_empty() {
    return Ok(false);
  }
  let inspected = state
    .inner
    .docker_api
    .inspect_container(container_id, None::<InspectContainerOptions>)
    .await
    .map_err(|err| err.map_err_context(|| "InspectContainer"))?;
  let has_config_healthcheck = inspected
    .config
    .as_ref()
    .and_then(|config| config.healthcheck.as_ref())
    .is_some();
  let has_state_health = inspected
    .state
    .as_ref()
    .and_then(|container_state| container_state.health.as_ref())
    .is_some();
  Ok(has_config_healthcheck || has_state_health)
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
  if !has_healthcheck(&cargo, &runtime_instances, state).await? {
    return Ok(());
  }

  if action == "health_status: unhealthy" {
    let old_instances = instances
      .iter()
      .filter(|process| process.name.starts_with("tmp-"))
      .cloned()
      .collect::<Vec<_>>();
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
    ObjPsStatusDb::update_actual_status(
      kind_key,
      &ObjPsStatusKind::Fail,
      &state.inner.pool,
    )
    .await?;
    let reason = unhealthy_instances_reason(&runtime_instances, state)
      .await?
      .unwrap_or_else(|| "Cargo healthcheck reported unhealthy".to_owned());
    state.emit_error_native_action(
      &cargo,
      NativeEventAction::Start,
      Some(format!(
        "CargoStart: Cargo {} became unhealthy: {reason}",
        cargo.spec.cargo_key,
      )),
    );
    state
      .emit_normal_native_action_sync(&cargo, NativeEventAction::Unhealthy)
      .await;
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
    let needed = health_quorum(runtime_instances.len());
    if healthy < needed {
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
  if kind != EventMessageTypeEnum::CONTAINER {
    return Ok(());
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
          let has_hc = has_healthcheck(&cargo, &runtime_instances, state)
            .await?
            || container_id_has_healthcheck(&id, state).await?;
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
            let has_hc =
              has_healthcheck(&cargo, &runtime_instances, state).await?;
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
  let state = state.clone();
  rt::Arbiter::new().handle().spawn(async move {
    loop {
      let mut streams =
        state.inner.docker_api.events(None::<EventsOptions<String>>);
      log::info!("event::analyze_docker: stream connected");
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
