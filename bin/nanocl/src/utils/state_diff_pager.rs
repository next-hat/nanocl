use std::{
  ffi::OsStr,
  io::{self, IsTerminal, Write},
  process::{Command, Stdio},
  sync::atomic::{AtomicBool, Ordering},
};

use nanocl_error::io::{IoError, IoResult};

use crate::models::{StateDiff, StateDiffOpts};

static PAGER_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_active() -> bool {
  PAGER_ACTIVE.load(Ordering::SeqCst)
}

pub(crate) fn print(diff: &StateDiff, opts: &StateDiffOpts) -> IoResult<()> {
  let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();
  let term = std::env::var_os("TERM");
  if !should_page(opts, terminal, term.as_deref()) {
    return diff.write_to(&mut io::stdout().lock(), opts.json);
  }

  // Render before opening the pager, preserving silent output for empty diffs.
  let mut bytes = Vec::new();
  diff.write_to(&mut bytes, false)?;
  if bytes.is_empty() {
    return Ok(());
  }

  // Let the pager handle terminal signals until it has exited and been reaped.
  PAGER_ACTIVE.store(true, Ordering::SeqCst);
  let mut child = match pager_command().spawn() {
    Ok(child) => child,
    Err(_) => {
      PAGER_ACTIVE.store(false, Ordering::SeqCst);
      let mut stdout = io::stdout().lock();
      stdout.write_all(&bytes)?;
      stdout.flush()?;
      return Ok(());
    }
  };
  // Close stdin before waiting, so less can observe EOF even for a short diff.
  let written = match child.stdin.take() {
    Some(mut stdin) => stdin.write_all(&bytes),
    None => Err(io::Error::other("pager input unavailable")),
  };
  let status = child.wait();
  if status.is_err() {
    // Do not leave a pager owning the terminal if waiting unexpectedly fails.
    let _ = child.kill();
    let _ = child.wait();
  }
  PAGER_ACTIVE.store(false, Ordering::SeqCst);
  pager_result(written, status?.success())
}

fn should_page(
  opts: &StateDiffOpts,
  terminal: bool,
  term: Option<&OsStr>,
) -> bool {
  !opts.json && !opts.no_pager && terminal && term != Some(OsStr::new("dumb"))
}

fn pager_command() -> Command {
  let mut command = Command::new("less");
  command.args(["-R", "-X", "-+F"]).stdin(Stdio::piped());
  command
}

fn pager_result(written: io::Result<()>, success: bool) -> IoResult<()> {
  if !success {
    return Err(IoError::other("State diff", "pager exited unsuccessfully"));
  }
  match written {
    // Quitting before the whole diff is consumed is a normal pager exit.
    Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
    result => Ok(result?),
  }
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::*;

  #[test]
  fn paging_requires_interactive_text_and_respects_direct_output() {
    for flags in [vec![], vec!["--pager"]] {
      let opts =
        StateDiffOpts::try_parse_from(["diff"].into_iter().chain(flags))
          .unwrap();
      assert!(should_page(&opts, true, Some(OsStr::new("xterm"))));
      assert!(should_page(&opts, true, None));
      assert!(!should_page(&opts, false, Some(OsStr::new("xterm"))));
      assert!(!should_page(&opts, true, Some(OsStr::new("dumb"))));
    }
    for flag in ["--json", "--no-pager"] {
      let opts = StateDiffOpts::try_parse_from(["diff", flag]).unwrap();
      assert!(!should_page(&opts, true, Some(OsStr::new("xterm"))));
    }
  }

  #[test]
  fn pager_preserves_colors_and_keeps_short_diffs_open_by_default() {
    let command = pager_command();
    assert_eq!(command.get_program(), "less");
    assert_eq!(
      command.get_args().collect::<Vec<_>>(),
      ["-R", "-X", "-+F"].map(OsStr::new),
    );
  }

  #[test]
  fn early_quit_is_success_but_write_and_pager_failures_are_errors() {
    assert!(pager_result(Ok(()), true).is_ok());
    assert!(pager_result(Err(io::ErrorKind::BrokenPipe.into()), true).is_ok());
    assert_eq!(
      pager_result(Err(io::ErrorKind::PermissionDenied.into()), true)
        .unwrap_err()
        .inner
        .kind(),
      io::ErrorKind::PermissionDenied,
    );
    assert!(pager_result(Ok(()), false).is_err());
    assert!(
      pager_result(Err(io::ErrorKind::BrokenPipe.into()), false).is_err()
    );
  }
}
