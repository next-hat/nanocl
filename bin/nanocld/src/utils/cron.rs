use std::{
  io::{Read, Write},
  process::Stdio,
};

use ntex::web;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{cron::validate_schedule, generic::GenericFilter, job::Job};

use crate::{
  models::{JobDb, SystemState},
  repositories::generic::RepositoryReadByTransform,
  vars,
};

/// Format the cron job command to start a job at a given time
fn format_cron_job_command(job: &Job, hosts: &[String]) -> String {
  let host = hosts
    .first()
    .cloned()
    .unwrap_or("unix:///run/nanocl/nanocl.sock".to_owned())
    .replace("unix://", "");
  format!(
    "curl -X POST --unix {host} http://localhost/v{}/processes/job/{}/start?incarnation={}",
    vars::VERSION,
    &job.spec.name,
    job.incarnation_key,
  )
}

static CRONTAB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The blocking owner retains serialization even if its caller is cancelled.
async fn serialized_update(
  update: impl FnOnce() -> IoResult<()> + Send + Sync + 'static,
) -> IoResult<()> {
  let serialized = CRONTAB_LOCK.lock().await;
  web::block(move || {
    let _serialized = serialized;
    update()
  })
  .await?;
  Ok(())
}

/// Drain the whole pipe while retaining only a bounded, plain-text diagnostic.
fn read_crontab_stderr(mut stderr: impl Read) -> std::io::Result<String> {
  const LIMIT: usize = 8 * 1024;
  let mut retained = Vec::with_capacity(LIMIT);
  let mut buffer = [0; 4096];
  let mut truncated = false;
  loop {
    let count = match stderr.read(&mut buffer) {
      Ok(0) => break,
      Ok(count) => count,
      Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
      Err(error) => return Err(error),
    };
    let keep = count.min(LIMIT - retained.len());
    retained.extend_from_slice(&buffer[..keep]);
    truncated |= keep < count;
  }
  let diagnostic = String::from_utf8_lossy(&retained)
    .chars()
    .filter(|character| {
      !character.is_control() || matches!(character, '\n' | '\t')
    })
    .collect::<String>();
  let mut diagnostic = diagnostic.trim().to_owned();
  if truncated {
    diagnostic.push_str("\n[stderr truncated]");
  }
  Ok(diagnostic)
}

/// Install one complete crontab while retaining the child and its pipe reader.
fn install_crontab(
  content: &str,
  command: &mut std::process::Command,
) -> IoResult<()> {
  let mut child = command
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  let Some(stderr) = child.stderr.take() else {
    let _ = child.kill();
    let _ = child.wait();
    return Err(IoError::other("Cron job", "crontab stderr is unavailable"));
  };
  // Drain stderr before writing stdin: either pipe can otherwise fill while
  // the other side is blocked. This thread belongs to the serialized owner.
  let diagnostic = match std::thread::Builder::new()
    .name("nanocl-crontab-stderr".to_owned())
    .spawn(move || read_crontab_stderr(stderr))
  {
    Ok(reader) => reader,
    Err(error) => {
      let _ = child.kill();
      let _ = child.wait();
      return Err(*error.map_err_context(|| "Cron job"));
    }
  };
  // Close stdin before waiting, and retain ownership on write failure too.
  let written = child
    .stdin
    .take()
    .ok_or_else(|| std::io::Error::other("crontab stdin is unavailable"))
    .and_then(|mut stdin| stdin.write_all(content.as_bytes()));
  if written.is_err() {
    let _ = child.kill();
  }
  let status = child.wait();
  if status.is_err() {
    let _ = child.kill();
    let _ = child.wait();
  }
  let diagnostic = match diagnostic.join() {
    Ok(Ok(diagnostic)) if diagnostic.is_empty() => String::new(),
    Ok(Ok(diagnostic)) => format!(": {diagnostic}"),
    Ok(Err(error)) => format!("; crontab stderr unavailable: {error}"),
    Err(_) => "; crontab stderr reader stopped unexpectedly".to_owned(),
  };
  written.map_err(|error| {
    IoError::with_context(
      "Cron job",
      std::io::Error::new(error.kind(), format!("{error}{diagnostic}")),
    )
  })?;
  let status = status.map_err(|error| {
    IoError::with_context(
      "Cron job",
      std::io::Error::new(error.kind(), format!("{error}{diagnostic}")),
    )
  })?;
  if !status.success() {
    return Err(IoError::interrupted(
      "Cron job",
      &format!("crontab exited with status {status}{diagnostic}"),
    ));
  }
  Ok(())
}

/// Read, modify, and install under one lock shared by all scheduled Jobs.
async fn update_crontab(
  update: impl FnOnce(String) -> String + Send + Sync + 'static,
) -> IoResult<()> {
  serialized_update(move || {
    let content = match std::fs::read_to_string("/var/spool/cron/crontabs/root")
    {
      Ok(content) => content,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
      Err(err) => return Err(*err.map_err_context(|| "Cron job")),
    };
    let content = update(content);
    install_crontab(&content, std::process::Command::new("crontab").arg("-"))
  })
  .await
}

/// Replace every occurrence of this exact incarnation-scoped command.
fn replace_cron_rule(content: &str, command: &str, rule: &str) -> String {
  let mut lines = content
    .lines()
    .filter(|line| {
      !line
        .trim_end()
        .strip_suffix(command)
        .is_some_and(|schedule| validate_schedule(schedule.trim_end()).is_ok())
    })
    .collect::<Vec<_>>();
  lines.push(rule);
  format!("{}\n", lines.join("\n"))
}

/// Recognize only the schedule and exact command shape generated by this daemon.
fn is_managed_cron_rule(line: &str) -> bool {
  let Some((schedule, command)) = line.split_once(" curl -X POST --unix ")
  else {
    return false;
  };
  if validate_schedule(schedule).is_err() {
    return false;
  }
  let Some((_, url)) = command.split_once(' ') else {
    return false;
  };
  let prefix = format!("http://localhost/v{}/processes/job/", vars::VERSION);
  let Some(target) = url.strip_prefix(&prefix) else {
    return false;
  };
  let Some((name, incarnation)) = target.split_once("/start?incarnation=")
  else {
    return false;
  };
  !name.is_empty() && uuid::Uuid::parse_str(incarnation).is_ok()
}

fn rebuild_crontab(content: &str, rules: &[String]) -> String {
  let mut lines = content
    .lines()
    .filter(|line| !is_managed_cron_rule(line))
    .collect::<Vec<_>>();
  lines.extend(rules.iter().map(String::as_str));
  if lines.is_empty() {
    String::new()
  } else {
    format!("{}\n", lines.join("\n"))
  }
}

fn scheduled_cron_rule(
  job: &Job,
  hosts: &[String],
) -> IoResult<Option<String>> {
  let Some(schedule) = &job.spec.schedule else {
    return Ok(None);
  };
  validate_schedule(schedule)
    .map_err(|err| IoError::invalid_input("Job schedule", &err))?;
  Ok(Some(format!(
    "{schedule} {}",
    format_cron_job_command(job, hosts)
  )))
}

async fn persisted_cron_rules(state: &SystemState) -> IoResult<Vec<String>> {
  const PAGE_SIZE: usize = 100;
  let mut offset = 0;
  let mut rules = Vec::new();
  loop {
    let filter = GenericFilter {
      limit: Some(PAGE_SIZE),
      offset: Some(offset),
      order_by: Some(vec!["key asc".to_owned()]),
      ..Default::default()
    };
    let jobs = JobDb::transform_read_by(&filter, &state.inner.pool).await?;
    let count = jobs.len();
    for job in jobs {
      if let Some(rule) = scheduled_cron_rule(&job, &state.inner.config.hosts)?
      {
        rules.push(rule);
      }
    }
    if count < PAGE_SIZE {
      return Ok(rules);
    }
    offset += count;
  }
}

/// Reconstruct local cron state before starting crond, recovery, or the API.
/// Existing execution identity and admission remain enforced by the start API.
pub async fn restore_cron_rules(state: &SystemState) -> IoResult<()> {
  let rules = persisted_cron_rules(state).await?;
  update_crontab(move |content| rebuild_crontab(&content, &rules)).await
}

/// Add a cron rule to the crontab to start a job at a given time
pub async fn add_cron_rule(
  item: &Job,
  schedule: &str,
  state: &SystemState,
) -> IoResult<()> {
  validate_schedule(schedule)
    .map_err(|err| IoError::invalid_input("Job schedule", &err))?;
  let cmd = format_cron_job_command(item, &state.inner.config.hosts);
  let cron_rule = format!("{} {cmd}", schedule);
  log::debug!("Creating cron rule: {cron_rule}");
  update_crontab(move |content| replace_cron_rule(&content, &cmd, &cron_rule))
    .await
}

/// Remove a cron rule from the crontab for the given job
pub async fn remove_cron_rule(item: &Job, state: &SystemState) -> IoResult<()> {
  let cmd = format_cron_job_command(item, &state.inner.config.hosts);
  log::debug!("Removing cron rule: {cmd}");
  update_crontab(move |content| {
    let content = content
      .lines()
      .filter(|line| !line.contains(&cmd))
      .collect::<Vec<_>>()
      .join("\n");
    format!("{content}\n")
  })
  .await
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scheduled_job(
    name: &str,
    incarnation: u128,
    schedule: Option<&str>,
  ) -> Job {
    Job {
      incarnation_key: uuid::Uuid::from_u128(incarnation),
      spec: nanocl_stubs::job::JobSpec {
        name: name.to_owned(),
        schedule: schedule.map(str::to_owned),
        ..Default::default()
      },
      ..Default::default()
    }
  }

  #[test]
  fn cron_registration_is_idempotent_for_the_exact_incarnation() {
    let job = scheduled_job("scheduled", 1, Some("@daily"));
    let other = scheduled_job("scheduled", 2, Some("@hourly"));
    let command = format_cron_job_command(&job, &[]);
    let rule = scheduled_cron_rule(&job, &[]).unwrap().unwrap();
    let other_rule = scheduled_cron_rule(&other, &[]).unwrap().unwrap();
    let existing = format!(
      "# retained\n# {command}\n{other_rule}\n@hourly {command}\n@daily {command}\n"
    );
    let replaced = replace_cron_rule(&existing, &command, &rule);
    assert_eq!(
      replaced,
      format!("# retained\n# {command}\n{other_rule}\n{rule}\n")
    );
    assert_eq!(replace_cron_rule(&replaced, &command, &rule), replaced);
  }

  #[test]
  fn startup_crontab_rebuild_preserves_unrelated_lines_and_current_identity() {
    let old = scheduled_job("scheduled", 1, Some("@daily"));
    let current = scheduled_job("scheduled", 2, Some("*/5 * * * *"));
    let deleted = scheduled_job("deleted", 3, Some("@reboot"));
    let old_rule = scheduled_cron_rule(&old, &[]).unwrap().unwrap();
    let deleted_rule = scheduled_cron_rule(&deleted, &[]).unwrap().unwrap();
    let rule =
      scheduled_cron_rule(&current, &["unix:///custom/nanocl.sock".into()])
        .unwrap()
        .unwrap();
    let unrelated = "# operator jobs\nMAILTO=root\n0 0 * * * echo retained\n";
    let existing =
      format!("{unrelated}{old_rule}\n{deleted_rule}\n{old_rule}\n");
    let rebuilt = rebuild_crontab(&existing, &[rule.clone()]);
    assert_eq!(rebuilt, format!("{unrelated}{rule}\n"));
    assert_eq!(rebuild_crontab(&rebuilt, &[rule.clone()]), rebuilt);
    assert_eq!(rebuild_crontab("", &[rule.clone()]), format!("{rule}\n"));
    assert_eq!(rebuild_crontab(&rebuilt, &[]), unrelated);
    assert!(rule.contains("--unix /custom/nanocl.sock"));
    assert!(
      rule.ends_with(&format!("incarnation={}", current.incarnation_key))
    );
  }

  #[test]
  fn startup_cron_rules_skip_unscheduled_jobs_and_reject_invalid_schedules() {
    assert!(
      scheduled_cron_rule(&scheduled_job("manual", 1, None), &[])
        .unwrap()
        .is_none()
    );
    assert!(
      scheduled_cron_rule(&scheduled_job("invalid", 2, Some("invalid")), &[])
        .is_err()
    );
    let job = scheduled_job("scheduled", 3, Some("@daily"));
    let rule = scheduled_cron_rule(&job, &[]).unwrap().unwrap();
    assert!(is_managed_cron_rule(&rule));
    assert!(!is_managed_cron_rule(&format!("# {rule}")));
    assert!(!is_managed_cron_rule(&format!("{rule} && echo unrelated")));
  }

  // Database-backed CI regression: reads declarations without installing cron
  // rules or launching executions. Do not run on the developer workstation.
  #[ntex::test]
  async fn startup_cron_rules_read_every_persisted_job_page() {
    fn no_routes(_: &mut ntex::web::ServiceConfig<SystemState>) {}
    let system =
      crate::utils::tests::gen_test_system(no_routes, vars::VERSION).await;
    let prefix = format!("cron-restore-{}", uuid::Uuid::new_v4().simple());
    let mut jobs = Vec::new();
    for index in 0..101 {
      let partial = nanocl_stubs::job::JobPartial {
        name: format!("{prefix}-{index:03}"),
        schedule: Some("@daily".into()),
        ..Default::default()
      };
      jobs.push(
        JobDb::create_with_spec(
          &partial,
          vars::VERSION,
          &system.state.inner.pool,
        )
        .await
        .unwrap(),
      );
    }
    let rules = persisted_cron_rules(&system.state).await.unwrap();
    for job in &jobs {
      let expected = scheduled_cron_rule(job, &system.state.inner.config.hosts)
        .unwrap()
        .unwrap();
      assert_eq!(rules.iter().filter(|rule| **rule == expected).count(), 1);
    }
    for job in jobs {
      JobDb::rollback_creation(&job, (), &system.state.inner.pool)
        .await
        .unwrap();
    }
    system.state.wait_event_loop().await;
  }

  // These subprocess regressions are for the existing CI suite. They never run
  // the real crontab command or modify the host environment or spool.
  fn crontab_fixture(script: &str) -> std::process::Command {
    let mut command = std::process::Command::new("timeout");
    command.args(["-k", "2", "10", "sh", "-c", script]);
    command
  }

  #[test]
  fn crontab_install_drains_stderr_while_writing_full_input() {
    let content = "x".repeat(4 * 1024 * 1024);
    let mut command = crontab_fixture(
      r#"
        set -e
        i=0
        while [ "$i" -lt 1024 ]; do
          printf '%4096s' '' >&2
          i=$((i + 1))
        done
        received=$(wc -c)
        test "$received" -eq 4194304
      "#,
    );
    // Both pipes exceed their capacities. Without concurrent drainage this
    // fails through the fixture's deadline instead of hanging the CI worker.
    install_crontab(&content, &mut command).unwrap();
  }

  #[test]
  fn crontab_install_reports_failed_status_and_bounded_plain_text() {
    let mut command = crontab_fixture(
      r#"
        printf '\033[31minvalid schedule\000\007\n' >&2
        i=0
        while [ "$i" -lt 1024 ]; do
          printf '%4096s' x >&2
          i=$((i + 1))
        done
        cat >/dev/null
        exit 7
      "#,
    );
    let error =
      install_crontab("* * * * * fixture\n", &mut command).unwrap_err();
    assert_eq!(error.context(), Some("Cron job"));
    assert_eq!(error.inner.kind(), std::io::ErrorKind::Interrupted);
    let message = error.to_string();
    assert!(message.contains("exit status: 7"), "{message}");
    assert!(message.contains("[31minvalid schedule"), "{message}");
    assert!(message.contains("[stderr truncated]"), "{message}");
    assert!(message.len() < 9 * 1024);
    assert!(!message.chars().any(|character| {
      character.is_control() && !matches!(character, '\n' | '\t')
    }));
  }

  #[test]
  fn crontab_install_retains_diagnostic_when_child_closes_input() {
    let mut command = crontab_fixture(
      r#"
        printf 'crontab refused input\n' >&2
        exec 0<&-
        exit 0
      "#,
    );
    // The diagnostic precedes closing stdin, so the parent's write-error kill
    // cannot race ahead of its production.
    let error =
      install_crontab(&"x".repeat(4 * 1024 * 1024), &mut command).unwrap_err();
    assert_eq!(error.context(), Some("Cron job"));
    assert_eq!(error.inner.kind(), std::io::ErrorKind::BrokenPipe);
    assert!(error.to_string().contains("crontab refused input"));
  }

  #[test]
  fn crontab_install_reports_missing_executable() {
    let missing = std::env::temp_dir()
      .join(format!("nanocl-crontab-missing-{}", uuid::Uuid::new_v4(),));
    let error = install_crontab("", &mut std::process::Command::new(missing))
      .unwrap_err();
    assert_eq!(error.context(), Some("Cron job"));
    assert_eq!(error.inner.kind(), std::io::ErrorKind::NotFound);
  }

  #[ntex::test]
  async fn cron_install_failure_releases_serialized_owner() {
    let error =
      serialized_update(|| install_crontab("", &mut crontab_fixture("exit 7")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("exit status: 7"));
    // A failed installation must release the owner so the next update can
    // install successfully. The existing cancellation test covers the inverse.
    ntex::time::timeout(
      std::time::Duration::from_secs(15),
      serialized_update(|| {
        install_crontab(
          "replacement\n",
          &mut crontab_fixture(
            "IFS= read -r value; test \"$value\" = replacement",
          ),
        )
      }),
    )
    .await
    .expect("failed installer retained the serialization lock")
    .unwrap();
  }

  #[test]
  fn crontab_stderr_is_bounded_drained_and_plain_text() {
    let input = vec![b'x'; 16 * 1024];
    let mut stderr = std::io::Cursor::new(input);
    let diagnostic = read_crontab_stderr(&mut stderr).unwrap();
    assert_eq!(
      diagnostic,
      format!("{}\n[stderr truncated]", "x".repeat(8 * 1024))
    );
    assert_eq!(stderr.position(), 16 * 1024);

    let diagnostic = read_crontab_stderr(std::io::Cursor::new(
      b"\x1b[31mparse error\0\r\n\tbad minute\x07\xff",
    ))
    .unwrap();
    assert_eq!(diagnostic, "[31mparse error\n\tbad minute\u{fffd}");
    assert!(
      read_crontab_stderr(std::io::Cursor::new(b"\0\r\x1b"))
        .unwrap()
        .is_empty()
    );
  }

  #[ntex::test]
  async fn cron_update_keeps_lock_after_caller_cancellation() {
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    let held = std::sync::Mutex::new(held);
    let mut first = Box::pin(serialized_update(move || {
      let _ = entered.send(());
      // Dropping the sender also releases this closure if an assertion fails.
      let _ = held.lock().unwrap().recv();
      Ok(())
    }));
    assert!(futures::poll!(first.as_mut()).is_pending());
    started.await.unwrap();
    drop(first);
    // Cancellation must not let another updater read the old crontab while
    // the blocking owner is still in the middle of installing its changes.
    assert!(CRONTAB_LOCK.try_lock().is_err());
    let mut second = Box::pin(serialized_update(|| Ok(())));
    assert!(futures::poll!(second.as_mut()).is_pending());
    release.send(()).unwrap();
    second.await.unwrap();
    assert!(CRONTAB_LOCK.try_lock().is_ok());
  }
}
