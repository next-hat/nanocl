use std::{
  io::{Read, Write},
  process::Stdio,
};

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
