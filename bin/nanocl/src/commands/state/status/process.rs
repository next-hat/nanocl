use bollard_next::models::HealthStatusEnum;
use nanocld_client::stubs::process::Process;

/// Latest observed process failure without exposing health-check log output.
pub(super) fn recent_failure(
  processes: &[Process],
  since: chrono::NaiveDateTime,
) -> Option<(chrono::NaiveDateTime, String)> {
  processes
    .iter()
    .filter(|process| process.updated_at >= since)
    .filter_map(|process| {
      let state = process.data.state.as_ref()?;
      let reason = if state.oom_killed == Some(true) {
        "out of memory (OOM killed)".to_owned()
      } else if let Some(error) = state
        .error
        .as_deref()
        .filter(|error| !error.trim().is_empty())
      {
        error.trim().to_owned()
      } else if state.running != Some(true)
        && let Some(code) = state.exit_code.filter(|code| *code != 0)
      {
        format!("exited with code {code}")
      } else if let Some(health) = state
        .health
        .as_ref()
        .filter(|health| health.status == Some(HealthStatusEnum::UNHEALTHY))
      {
        let code = health.log.as_ref().and_then(|entries| {
          entries
            .iter()
            .rev()
            .find_map(|entry| entry.exit_code.filter(|code| *code != 0))
        });
        match code {
          Some(code) => {
            format!("unhealthy (health check exited with code {code})")
          }
          None => "unhealthy".to_owned(),
        }
      } else {
        return None;
      };
      Some((process.updated_at, format!("{}: {reason}", process.name)))
    })
    .max_by_key(|(updated_at, _)| *updated_at)
}

#[cfg(test)]
mod tests {
  use bollard_next::models::{
    ContainerInspectResponse, ContainerState, Health, HealthcheckResult,
  };
  use nanocld_client::stubs::process::ProcessKind;

  use super::*;

  fn process(name: &str, seconds: i64, state: ContainerState) -> Process {
    let updated_at = chrono::DateTime::from_timestamp(seconds, 0)
      .unwrap()
      .naive_utc();
    Process {
      key: name.to_owned(),
      created_at: updated_at,
      updated_at,
      name: name.to_owned(),
      kind: ProcessKind::Cargo,
      node_name: "node-a".to_owned(),
      kind_key: "global.app".to_owned(),
      ip_address: None,
      data: ContainerInspectResponse {
        state: Some(state),
        ..Default::default()
      },
    }
  }

  #[test]
  fn status_process_failure_selects_newest_observation_within_window() {
    let old = process(
      "init",
      10,
      ContainerState {
        exit_code: Some(2),
        ..Default::default()
      },
    );
    let newest = process(
      "app",
      20,
      ContainerState {
        exit_code: Some(3),
        ..Default::default()
      },
    );
    let since = old.updated_at;
    let result = recent_failure(&[newest.clone(), old], since).unwrap();
    assert_eq!(
      result,
      (newest.updated_at, "app: exited with code 3".to_owned())
    );
    assert!(
      recent_failure(
        &[newest.clone()],
        newest.updated_at + chrono::Duration::seconds(1)
      )
      .is_none()
    );
  }

  #[test]
  fn status_process_failure_prioritizes_oom_and_runtime_error() {
    let mut failed = process(
      "init",
      10,
      ContainerState {
        oom_killed: Some(true),
        error: Some("cannot start".to_owned()),
        exit_code: Some(137),
        ..Default::default()
      },
    );
    assert_eq!(
      recent_failure(&[failed.clone()], failed.updated_at)
        .unwrap()
        .1,
      "init: out of memory (OOM killed)"
    );
    failed.data.state.as_mut().unwrap().oom_killed = Some(false);
    assert_eq!(
      recent_failure(&[failed.clone()], failed.updated_at)
        .unwrap()
        .1,
      "init: cannot start"
    );
  }

  #[test]
  fn status_process_failure_ignores_success_and_previous_running_exit() {
    for (running, exit_code) in [(true, 7), (false, 0)] {
      let healthy = process(
        "app",
        10,
        ContainerState {
          running: Some(running),
          exit_code: Some(exit_code),
          error: Some("  ".to_owned()),
          ..Default::default()
        },
      );
      assert!(recent_failure(&[healthy.clone()], healthy.updated_at).is_none());
    }
  }

  #[test]
  fn status_process_failure_uses_last_failed_check_without_log_output() {
    let unhealthy = process(
      "optional",
      10,
      ContainerState {
        running: Some(true),
        health: Some(Health {
          status: Some(HealthStatusEnum::UNHEALTHY),
          log: Some(
            vec![1, 2, 0]
              .into_iter()
              .map(|code| HealthcheckResult {
                exit_code: Some(code),
                output: Some("private check output".to_owned()),
                ..Default::default()
              })
              .collect(),
          ),
          ..Default::default()
        }),
        ..Default::default()
      },
    );
    assert_eq!(
      recent_failure(&[unhealthy.clone()], unhealthy.updated_at)
        .unwrap()
        .1,
      "optional: unhealthy (health check exited with code 2)"
    );
  }
}
