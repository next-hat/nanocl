use crate::models::{StateOutput, StateOutputEvent};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use nanocl_error::io::IoResult;
use std::time::Duration;

const PLAIN_WAIT_INTERVAL: Duration = Duration::from_secs(30);

pub(super) fn plain_elapsed(elapsed: Duration) -> String {
  let seconds = elapsed.as_secs();
  if seconds >= 3600 {
    format!(
      "{}h {:02}m {:02}s",
      seconds / 3600,
      seconds / 60 % 60,
      seconds % 60
    )
  } else if seconds >= 60 {
    format!("{}m {:02}s", seconds / 60, seconds % 60)
  } else {
    format!("{seconds}s")
  }
}

fn plain_state_line(
  resource: &str,
  message: &str,
  elapsed: Duration,
  error: Option<&str>,
) -> String {
  let mut line =
    format!("  {resource}  {message} [{}]", plain_elapsed(elapsed));
  if let Some(error) = error {
    line.push_str(": ");
    line.push_str(error);
  }
  line
}

fn plain_phase_changed(previous: &str, message: &str) -> bool {
  !message.is_empty() && previous != message
}

fn plain_wait_message(
  phase: &str,
  elapsed: Duration,
  last_update: Duration,
) -> Option<String> {
  (elapsed.saturating_sub(last_update) >= PLAIN_WAIT_INTERVAL)
    .then(|| format!("{phase} (still waiting)"))
}

/// Keep the operation in this future so completion or cancellation also drops
/// its timer. No progress task can outlive the operation it describes.
async fn with_plain_state_progress<T>(
  pg: &ProgressBar,
  operation: impl std::future::Future<Output = T>,
) -> T {
  futures::pin_mut!(operation);
  let mut last_update = pg.elapsed();
  loop {
    // Construct the timer only when polled; a ready operation needs no timer.
    let waiting = async { tokio::time::sleep(PLAIN_WAIT_INTERVAL).await };
    futures::pin_mut!(waiting);
    match futures::future::select(operation.as_mut(), waiting).await {
      futures::future::Either::Left((result, _)) => return result,
      futures::future::Either::Right((_, _)) => {
        let elapsed = pg.elapsed();
        if let Some(message) =
          plain_wait_message(&pg.message(), elapsed, last_update)
        {
          eprintln!(
            "{}",
            plain_state_line(&pg.prefix(), &message, elapsed, None)
          );
          last_update = elapsed;
        }
      }
    }
  }
}

pub(crate) fn create_progress(msg: &str, style: &ProgressStyle) -> ProgressBar {
  let pg = ProgressBar::new(1);
  pg.enable_steady_tick(std::time::Duration::from_millis(50));
  pg.set_style(style.clone());
  pg.set_message(msg.to_owned());
  pg
}

pub(crate) fn create_spinner_style(key: &str, color: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!(
    "{{spinner:.{color}.bold}} {} {{msg}}",
    key.bold()
  ))
  .unwrap()
  .tick_strings(&[
    "▹▹▹▹▹",
    "▸▹▹▹▹",
    "▹▸▹▹▹",
    "▹▹▸▹▹",
    "▹▹▹▸▹",
    "▹▹▹▹▸",
    ">",
  ])
}

/// Create the overall state operation bar. Individual rows are inserted above it.
pub(crate) fn create_state_progress(
  total: u64,
  message: &'static str,
  output: Option<&StateOutput>,
) -> IoResult<(indicatif::MultiProgress, ProgressBar)> {
  let progress = indicatif::MultiProgress::new();
  if output.is_some() {
    progress.set_draw_target(indicatif::ProgressDrawTarget::hidden());
  }
  let summary = progress.add(ProgressBar::new(total));
  summary.set_style(
    ProgressStyle::with_template(
      "{spinner:.cyan} {msg:<12} [{bar:24.cyan/dim}] {pos}/{len} [{elapsed_precise}]",
    )
    .unwrap()
    .progress_chars("=>-")
  );
  summary.set_message(message);
  summary.enable_steady_tick(std::time::Duration::from_millis(100));
  if let Some(output) = output {
    output.emit(StateOutputEvent::State {
      status: "started",
      completed: 0,
      total,
      success: None,
      elapsed_ms: 0,
    })?;
  }
  Ok((progress, summary))
}

pub(crate) fn create_state_item(
  progress: &indicatif::MultiProgress,
  summary: &ProgressBar,
  token: &str,
  output: Option<&StateOutput>,
) -> IoResult<ProgressBar> {
  let pg = progress.insert_before(summary, ProgressBar::new_spinner());
  pg.set_style(
    ProgressStyle::with_template(
      "  {spinner:.cyan} {prefix:<48.bold}  {msg:<10} [{elapsed_precise:.dim}]",
    )
    .unwrap(),
  );
  pg.set_prefix(token.to_owned());
  set_state_message(&pg, summary, "Submitting", output)?;
  pg.enable_steady_tick(std::time::Duration::from_millis(100));
  Ok(pg)
}

pub(crate) fn set_state_message(
  pg: &ProgressBar,
  summary: &ProgressBar,
  message: &str,
  output: Option<&StateOutput>,
) -> IoResult<()> {
  let changed = plain_phase_changed(&pg.message(), message);
  pg.set_message(message.to_owned());
  if let Some(output) = output {
    output.emit(StateOutputEvent::Item {
      resource: &pg.prefix(),
      status: &message.to_ascii_lowercase(),
      completed: summary.position(),
      total: summary.length().unwrap_or(0),
      success: None,
      elapsed_ms: pg.elapsed().as_millis() as u64,
      error: None,
    })?;
  } else if pg.is_hidden() && changed {
    eprintln!(
      "{}",
      plain_state_line(&pg.prefix(), message, pg.elapsed(), None)
    );
  }
  Ok(())
}

pub(crate) fn finish_state_item(
  pg: &ProgressBar,
  summary: &ProgressBar,
  status: &str,
  failed: bool,
  error: Option<&str>,
  output: Option<&StateOutput>,
) -> IoResult<()> {
  let template = if failed {
    "    {prefix:<48.bold}  {msg:<10.red} [{elapsed_precise:.dim}]"
  } else {
    "    {prefix:<48.bold}  {msg:<10.green} [{elapsed_precise:.dim}]"
  };
  pg.set_style(ProgressStyle::with_template(template).unwrap());
  pg.finish_with_message(status.to_owned());
  summary.inc(1);
  if let Some(output) = output {
    output.emit(StateOutputEvent::Item {
      resource: &pg.prefix(),
      status: &status.to_ascii_lowercase(),
      completed: summary.position(),
      total: summary.length().unwrap_or(0),
      success: Some(!failed),
      elapsed_ms: pg.elapsed().as_millis() as u64,
      error,
    })?;
  } else if pg.is_hidden() {
    eprintln!(
      "{}",
      plain_state_line(&pg.prefix(), status, pg.elapsed(), error)
    );
  }
  Ok(())
}

pub(crate) fn finish_state_progress(
  summary: &ProgressBar,
  message: &'static str,
  failed: bool,
  output: Option<&StateOutput>,
) -> IoResult<()> {
  let template = if failed {
    "{msg:<12.red.bold} [{bar:24.red/dim}] {pos}/{len} [{elapsed_precise}]"
  } else {
    "{msg:<12.green.bold} [{bar:24.green/dim}] {pos}/{len} [{elapsed_precise}]"
  };
  summary.set_style(
    ProgressStyle::with_template(template)
      .unwrap()
      .progress_chars("=>-"),
  );
  if failed {
    // Preserve the actual count when an apply stops before processing all items.
    summary.abandon_with_message(message);
  } else {
    summary.finish_with_message(message);
  }
  if let Some(output) = output {
    output.emit(StateOutputEvent::State {
      status: "completed",
      completed: summary.position(),
      total: summary.length().unwrap_or(0),
      success: Some(!failed),
      elapsed_ms: summary.elapsed().as_millis() as u64,
    })?;
  } else if summary.is_hidden() {
    eprintln!(
      "{message}: {}/{} [{}]",
      summary.position(),
      summary.length().unwrap_or(0),
      plain_elapsed(summary.elapsed())
    );
  }
  Ok(())
}

pub(crate) async fn run_state_step<F, Fut>(
  progress: &indicatif::MultiProgress,
  summary: &ProgressBar,
  token: &str,
  output: Option<&StateOutput>,
  image_actor: Option<(
    &nanocld_client::NanocldClient,
    &str,
    nanocld_client::stubs::system::EventActorKind,
  )>,
  operation: F,
) -> nanocl_error::io::IoResult<()>
where
  F: FnOnce(ProgressBar) -> Fut,
  Fut: std::future::Future<Output = nanocl_error::io::IoResult<&'static str>>,
{
  let pg = create_state_item(progress, summary, token, output)?;
  let operation = operation(pg.clone());
  let operation = async {
    match image_actor {
      Some((client, key, kind)) => {
        super::state_progress::with_image_progress(
          client,
          key,
          kind,
          (progress, summary),
          &pg,
          output,
          operation,
        )
        .await
      }
      None => operation.await,
    }
  };
  let result = if output.is_none() && pg.is_hidden() {
    with_plain_state_progress(&pg, operation).await
  } else {
    operation.await
  };
  match &result {
    Ok(status) => finish_state_item(&pg, summary, status, false, None, output)?,
    Err(err) => finish_state_item(
      &pg,
      summary,
      "Failed",
      true,
      Some(&err.to_string()),
      output,
    )?,
  }
  result.map(|_| ())
}

#[cfg(test)]
mod tests {
  use super::*;
  use nanocl_error::io::IoError;

  #[test]
  fn plain_progress_formats_elapsed_boundaries() {
    for (seconds, expected) in [
      (0, "0s"),
      (59, "59s"),
      (60, "1m 00s"),
      (3599, "59m 59s"),
      (3600, "1h 00m 00s"),
      (3723, "1h 02m 03s"),
    ] {
      assert_eq!(plain_elapsed(Duration::from_secs(seconds)), expected);
    }
  }

  #[test]
  fn plain_progress_lines_include_resource_phase_elapsed_and_error() {
    assert_eq!(
      plain_state_line(
        "cargo/global.app",
        "Starting",
        Duration::from_secs(4),
        None
      ),
      "  cargo/global.app  Starting [4s]",
    );
    assert_eq!(
      plain_state_line(
        "vm/global.worker",
        "Failed",
        Duration::from_secs(62),
        Some("image unavailable")
      ),
      "  vm/global.worker  Failed [1m 02s]: image unavailable",
    );
  }

  #[test]
  fn plain_progress_deduplicates_only_identical_phases() {
    assert!(plain_phase_changed("", "Submitting"));
    assert!(!plain_phase_changed("Applying", "Applying"));
    assert!(plain_phase_changed("Applying", "Starting"));
    assert!(plain_phase_changed("Starting", "Applying"));
    assert!(!plain_phase_changed("Starting", ""));
  }

  #[test]
  fn plain_progress_waiting_updates_are_spaced_thirty_seconds_apart() {
    for seconds in [0, 29, 30, 44] {
      assert!(
        plain_wait_message(
          "Starting",
          Duration::from_secs(seconds),
          Duration::from_secs(15)
        )
        .is_none()
      );
    }
    assert_eq!(
      plain_wait_message(
        "Starting",
        Duration::from_secs(45),
        Duration::from_secs(15)
      ),
      Some("Starting (still waiting)".to_owned()),
    );
    assert!(
      plain_wait_message(
        "Starting",
        Duration::from_secs(74),
        Duration::from_secs(45)
      )
      .is_none()
    );
    assert!(
      plain_wait_message(
        "Starting",
        Duration::from_secs(75),
        Duration::from_secs(45)
      )
      .is_some()
    );
  }

  #[test]
  fn plain_progress_ready_operation_needs_no_timer_runtime() {
    let pg = ProgressBar::hidden();
    assert_eq!(
      futures::executor::block_on(with_plain_state_progress(
        &pg,
        std::future::ready("Ready")
      )),
      "Ready",
    );
  }

  #[ntex::test]
  async fn plain_progress_cancellation_drops_the_pending_operation() {
    use std::{cell::Cell, rc::Rc};

    struct Dropped(Rc<Cell<bool>>);
    impl Drop for Dropped {
      fn drop(&mut self) {
        self.0.set(true);
      }
    }

    let dropped = Rc::new(Cell::new(false));
    let guard = Dropped(dropped.clone());
    let pg = ProgressBar::hidden();
    let mut waiting = Box::pin(with_plain_state_progress(&pg, async move {
      let _guard = guard;
      std::future::pending::<()>().await;
    }));
    // Polling pending work also exercises the timer on the CLI's runtime.
    assert!(futures::poll!(waiting.as_mut()).is_pending());
    assert!(!dropped.get());
    drop(waiting);
    assert!(dropped.get());
  }

  #[test]
  fn state_progress_preserves_partial_count_on_failure() {
    let progress = indicatif::MultiProgress::with_draw_target(
      indicatif::ProgressDrawTarget::hidden(),
    );
    let summary = progress.add(ProgressBar::new(3));
    let result = futures::executor::block_on(run_state_step(
      &progress,
      &summary,
      "cargo/global.app",
      None,
      None,
      |_| async { Err(IoError::other("Apply", "image unavailable")) },
    ));
    assert!(result.is_err());
    finish_state_progress(&summary, "Apply failed", true, None).unwrap();
    assert_eq!(summary.position(), 1);
    assert_eq!(summary.length(), Some(3));
    assert_eq!(summary.message(), "Apply failed");
    assert!(summary.is_finished());
  }
}
