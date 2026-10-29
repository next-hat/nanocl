use std::{io::Write, process::Stdio};

use ntex::web;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{cron::validate_schedule, job::Job};

use crate::{models::SystemState, vars};

/// Format the cron job command to start a job at a given time
fn format_cron_job_command(job: &Job, state: &SystemState) -> String {
  let host = state
    .inner
    .config
    .hosts
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

/// Read, modify, and install under one lock shared by all scheduled Jobs.
async fn update_crontab(
  update: impl FnOnce(String) -> String + Send + Sync + 'static,
) -> IoResult<()> {
  serialized_update(move || {
    let content = std::fs::read_to_string("/var/spool/cron/crontabs/root")
      .map_err(|err| err.map_err_context(|| "Cron job"))?;
    let content = update(content);
    let mut child = std::process::Command::new("crontab")
      .arg("-")
      .stdin(Stdio::piped())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map_err(|err| err.map_err_context(|| "Cron job"))?;
    // Close stdin before waiting, and retain ownership on write failure too.
    let written = child
      .stdin
      .take()
      .ok_or_else(|| IoError::other("Cron job", "crontab stdin is unavailable"))
      .and_then(|mut stdin| {
        stdin
          .write_all(content.as_bytes())
          .map_err(|err| err.map_err_context(|| "Cron job"))?;
        Ok(())
      });
    if let Err(error) = written {
      let _ = child.kill();
      let _ = child.wait();
      return Err(error);
    }
    let status = child
      .wait()
      .map_err(|err| err.map_err_context(|| "Cron job"))?;
    if !status.success() {
      return Err(IoError::interrupted(
        "Cron job",
        &format!("crontab exited with status {status}"),
      ));
    }
    Ok(())
  })
  .await
}

/// Add a cron rule to the crontab to start a job at a given time
pub async fn add_cron_rule(
  item: &Job,
  schedule: &str,
  state: &SystemState,
) -> IoResult<()> {
  validate_schedule(schedule)
    .map_err(|err| IoError::invalid_input("Job schedule", &err))?;
  let cmd = format_cron_job_command(item, state);
  let cron_rule = format!("{} {cmd}", schedule);
  log::debug!("Creating cron rule: {cron_rule}");
  update_crontab(move |mut content| {
    if !content.is_empty() && !content.ends_with('\n') {
      content.push('\n');
    }
    content.push_str(&cron_rule);
    content.push('\n');
    content
  })
  .await
}

/// Remove a cron rule from the crontab for the given job
pub async fn remove_cron_rule(item: &Job, state: &SystemState) -> IoResult<()> {
  let cmd = format_cron_job_command(item, state);
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
