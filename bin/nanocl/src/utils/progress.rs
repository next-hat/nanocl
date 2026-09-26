use crate::models::{StateOutput, StateOutputEvent};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use nanocl_error::io::IoResult;

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
      "  {spinner:.cyan} {prefix:.bold}  {msg} [{elapsed_precise:.dim}]",
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
    "  {prefix:.bold}  {msg:.red} [{elapsed_precise:.dim}]"
  } else {
    "  {prefix:.bold}  {msg:.green} [{elapsed_precise:.dim}]"
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
    eprintln!("{}  {status}", pg.prefix());
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
      "{message}: {}/{}",
      summary.position(),
      summary.length().unwrap_or(0)
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
  let result = match image_actor {
    Some((client, key, kind)) => {
      super::state_progress::with_image_progress(
        client, key, kind, progress, summary, output, operation,
      )
      .await
    }
    None => operation.await,
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
