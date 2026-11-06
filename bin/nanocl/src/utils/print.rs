use futures::StreamExt;
use futures::stream::select_all;
use ntex::channel::mpsc::Receiver;
use tabled::Table;
use tabled::settings::object::Segment;
use tabled::settings::{Alignment, Modify, Padding, Style};

use nanocl_error::http::HttpError;
use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::process::{OutputKind, ProcessOutputLog};

use crate::models::DisplayFormat;

/// Use the terminal's current width, or a predictable width for redirected output.
pub(crate) fn terminal_width() -> usize {
  dialoguer::console::Term::stdout()
    .size_checked()
    .or_else(|| dialoguer::console::Term::stderr().size_checked())
    .map(|(_, columns)| usize::from(columns))
    .filter(|width| *width > 0)
    .or_else(|| {
      std::env::var("COLUMNS")
        .ok()?
        .parse::<usize>()
        .ok()
        .filter(|width| *width > 0)
    })
    .unwrap_or(80)
    .clamp(2, 1000)
}

/// Wrap human-readable details without dropping identifiers or splitting UTF-8.
/// Every line is indented; control sequences cannot alter the terminal layout.
/// Internal whitespace is retained, including separators at line breaks, so
/// quoted paths and other identities do not change when they are displayed.
pub(crate) fn wrap_lines(text: &str, width: usize, indent: usize) -> String {
  use dialoguer::console::{measure_text_width, strip_ansi_codes};

  let width = width.max(2);
  let indent = indent.min(width - 2);
  let available = width - indent;
  let prefix = " ".repeat(indent);
  let text = strip_ansi_codes(text)
    .chars()
    .map(|ch| {
      if ch.is_control() && ch != '\n' {
        ' '
      } else {
        ch
      }
    })
    .collect::<String>();
  let mut lines = Vec::new();
  for paragraph in text.split('\n') {
    // Callers supply the indentation separately. Remove it only once, before
    // wrapping, rather than stripping whitespace from every continuation.
    let mut remaining = paragraph.trim();
    if remaining.is_empty() {
      lines.push(String::new());
      continue;
    }
    while !remaining.is_empty() {
      let mut end = 0;
      let mut word_break = None;
      let mut has_word = false;
      for (index, ch) in remaining.char_indices() {
        let next = index + ch.len_utf8();
        if measure_text_width(&remaining[..next]) > available {
          if ch.is_whitespace() {
            word_break = Some(end);
          }
          break;
        }
        end = next;
        if ch.is_whitespace() && has_word {
          word_break = Some(end);
        } else if !ch.is_whitespace() {
          has_word = true;
        }
      }
      // Break after a separator when a word would overflow. Keep that
      // separator in the preceding line, within the measured width.
      let end = if end == 0 {
        // Always make progress even if a terminal reports less space than a
        // single displayed character needs.
        remaining.chars().next().unwrap().len_utf8()
      } else if end == remaining.len() {
        end
      } else {
        word_break.unwrap_or(end)
      };
      lines.push(format!("{prefix}{}", &remaining[..end]));
      remaining = &remaining[end..];
    }
  }
  lines.join("\n")
}

/// Print a table from an iterator of [Tabled](tabled::Tabled) elements
pub(crate) fn print_table<T>(iter: impl IntoIterator<Item = T>)
where
  T: tabled::Tabled,
{
  println!("{}", render_table(iter));
}

/// Render a table without writing to stdout.
pub(crate) fn render_table<T>(iter: impl IntoIterator<Item = T>) -> String
where
  T: tabled::Tabled,
{
  Table::new(iter)
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 4, 0, 0))
        .with(Alignment::left()),
    )
    .to_string()
}

/// Print yaml from a serializable data
pub(crate) fn print_yml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let yml = serde_yaml::to_string(&data)
    .map_err(|err| err.map_err_context(|| "Print yaml"))?;
  print!("{yml}");
  Ok(())
}

/// Print json from a serializable data
pub(crate) fn print_json<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let json = serde_json::to_string_pretty(&data)
    .map_err(|err| err.map_err_context(|| "Print json"))?;
  print!("{json}");
  Ok(())
}

/// Print toml from a serializable data
pub(crate) fn print_toml<T>(data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  let toml = toml::to_string(&data).map_err(|err| {
    IoError::with_context(
      "Print toml",
      std::io::Error::new(std::io::ErrorKind::InvalidData, err),
    )
  })?;
  print!("{toml}");
  Ok(())
}

/// Display data in a specific format
pub(crate) fn display_format<T>(format: &DisplayFormat, data: T) -> IoResult<()>
where
  T: serde::Serialize,
{
  match format {
    DisplayFormat::Yaml => print_yml(data),
    DisplayFormat::Toml => print_toml(data),
    DisplayFormat::Json => print_json(data),
  }
}

pub(crate) async fn logs_process_stream(
  stream: Receiver<Result<ProcessOutputLog, HttpError>>,
) -> IoResult<()> {
  let mut stream = stream;
  while let Some(s) = stream.next().await {
    let s = match s {
      Ok(s) => s,
      Err(e) => return Err(e.map_err_context(|| "Stream").into()),
    };
    let output = format!("[{}] {}", &s.name, &s.log.data);
    match s.log.kind {
      OutputKind::StdOut => {
        print!("{output}");
      }
      OutputKind::StdErr => {
        eprint!("{output}");
      }
      OutputKind::Console => print!("{output}"),
      _ => {}
    }
  }
  Ok(())
}

/// Merge and print logs from multiple process streams concurrently.
/// Each stream item is prefixed with the process name already by the server.
pub(crate) async fn logs_process_streams(
  streams: Vec<Receiver<Result<ProcessOutputLog, HttpError>>>,
) -> IoResult<()> {
  let mut merged = select_all(streams);
  while let Some(s) = merged.next().await {
    let s = match s {
      Ok(s) => s,
      Err(e) => return Err(e.map_err_context(|| "Stream").into()),
    };
    let output = format!("[{}] {}", &s.name, &s.log.data);
    match s.log.kind {
      OutputKind::StdOut => print!("{output}"),
      OutputKind::StdErr => eprint!("{output}"),
      OutputKind::Console => print!("{output}"),
      _ => {}
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use dialoguer::console::measure_text_width;

  use super::wrap_lines;

  #[test]
  fn detail_wrapping_preserves_long_unicode_identifiers() {
    let name = format!("production.{}e\u{301}", "服务".repeat(40));
    for width in [20, 40, 80, 100] {
      let output = wrap_lines(&name, width, 2);
      assert!(output.lines().all(|line| measure_text_width(line) <= width));
      assert_eq!(output.split_whitespace().collect::<String>(), name);
    }
  }

  #[test]
  fn detail_wrapping_keeps_words_and_sanitizes_terminal_controls() {
    let output =
      wrap_lines("\x1b[31mLast failure\x1b[0m\t2m ago:\nexit code 127", 20, 2);
    assert_eq!(
      output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n"),
      "  Last failure 2m\n  ago:\n  exit code 127"
    );
    assert_eq!(wrap_lines("", 80, 2), "");
  }

  #[test]
  fn detail_wrapping_preserves_internal_spaces_without_repeating_indent() {
    for text in [
      "Statefile \"./my  config.yml\"".to_owned(),
      format!("prefix{}suffix", " ".repeat(30)),
    ] {
      for width in [8, 20, 40, 80] {
        let output = wrap_lines(&format!("  {text}"), width, 2);
        assert!(output.lines().all(|line| measure_text_width(line) <= width));
        let restored = output
          .lines()
          .map(|line| line.strip_prefix("  ").unwrap())
          .collect::<String>();
        assert_eq!(restored, text);
      }
    }
    assert_eq!(
      wrap_lines("  Statefile \"./my  config.yml\"", 80, 2),
      "  Statefile \"./my  config.yml\""
    );
  }

  #[test]
  fn detail_wrapping_keeps_words_that_exactly_fill_the_line() {
    assert_eq!(
      wrap_lines("alpha beta gamma", 12, 2),
      "  alpha beta\n   gamma"
    );
    let output = wrap_lines("12345678 abcdefgh", 10, 2);
    assert!(output.lines().all(|line| !line.trim().is_empty()));
    assert!(output.lines().all(|line| measure_text_width(line) <= 10));
    assert_eq!(
      output
        .lines()
        .map(|line| line.strip_prefix("  ").unwrap())
        .collect::<String>(),
      "12345678 abcdefgh"
    );
  }
}
