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

/// Measure stdout only, so redirected output is independent of stderr's TTY.
pub(crate) fn terminal_width() -> usize {
  stream_width(dialoguer::console::Term::stdout())
}

/// Human apply summaries, receipts, and progress are written to stderr.
pub(crate) fn stderr_width() -> usize {
  stream_width(dialoguer::console::Term::stderr())
}

fn stream_width(terminal: dialoguer::console::Term) -> usize {
  resolve_width(
    terminal
      .size_checked()
      .map(|(_, columns)| usize::from(columns)),
    std::env::var("COLUMNS").ok().as_deref(),
  )
}

fn resolve_width(terminal: Option<usize>, columns: Option<&str>) -> usize {
  terminal
    .filter(|width| *width > 0)
    .or_else(|| columns?.parse::<usize>().ok().filter(|width| *width > 0))
    .unwrap_or(80)
    .clamp(2, 1000)
}

/// Wrap human-readable details without dropping identifiers or splitting UTF-8.
/// Every line is indented; control sequences cannot alter the terminal layout.
/// Internal whitespace is retained, including separators at line breaks, so
/// quoted paths and other identities do not change when they are displayed.
pub(crate) fn wrap_lines(text: &str, width: usize, indent: usize) -> String {
  use dialoguer::console::{measure_text_width, strip_ansi_codes};
  use unicode_segmentation::UnicodeSegmentation;

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
    let paragraph = paragraph.trim_start();
    if paragraph.is_empty() {
      lines.push(String::new());
      continue;
    }
    // Measure the complete paragraph only once. This preserves exact widths
    // for text that fits, including ligatures spanning grapheme boundaries.
    if measure_text_width(paragraph) <= available {
      lines.push(format!("{prefix}{paragraph}"));
      continue;
    }
    // Cache each extended grapheme's width once: measuring growing prefixes
    // is quadratic for long zero-width combining sequences. Graphemes keep
    // combining marks, flags, and ZWJ emoji intact. Their summed widths are a
    // conservative budget for cross-grapheme ligatures in overflowing text;
    // these may wrap earlier, while every character remains in the output.
    let mut units = Vec::new();
    for (index, grapheme) in paragraph.grapheme_indices(true) {
      let grapheme_width = measure_text_width(grapheme);
      if grapheme_width <= available {
        units.push((
          index + grapheme.len(),
          grapheme_width,
          grapheme.chars().next().is_some_and(char::is_whitespace),
        ));
      } else {
        // Some Indic conjunct chains form one arbitrarily wide grapheme.
        // Split only oversized clusters, caching each scalar's width once,
        // so these still fit without reintroducing growing-prefix scans.
        for (offset, ch) in grapheme.char_indices() {
          let end = offset + ch.len_utf8();
          units.push((
            index + end,
            measure_text_width(&grapheme[offset..end]),
            ch.is_whitespace(),
          ));
        }
      }
    }
    let mut start = 0;
    while start < units.len() {
      let mut end = start;
      let mut line_width = 0usize;
      let mut word_break = None;
      let mut has_word = false;
      for (index, &(_, unit_width, whitespace)) in
        units.iter().enumerate().skip(start)
      {
        if line_width.saturating_add(unit_width) > available {
          if whitespace {
            word_break = Some(end);
          }
          break;
        }
        end = index + 1;
        line_width += unit_width;
        if whitespace && has_word {
          word_break = Some(end);
        } else if !whitespace {
          has_word = true;
        }
      }
      // Break after a separator when a word would overflow. Keep that
      // separator in the preceding line, within the measured width.
      let end = if end == start {
        // Always make progress even if a terminal reports less space than a
        // single displayed scalar needs.
        start + 1
      } else if end == units.len() {
        end
      } else {
        word_break.unwrap_or(end)
      };
      let byte_start = if start == 0 { 0 } else { units[start - 1].0 };
      let byte_end = units[end - 1].0;
      lines.push(format!("{prefix}{}", &paragraph[byte_start..byte_end]));
      start = end;
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

  use super::{resolve_width, wrap_lines};

  #[test]
  fn redirected_output_uses_configured_width_or_predictable_default() {
    assert_eq!(resolve_width(None, Some("40")), 40);
    assert_eq!(resolve_width(None, None), 80);
    assert_eq!(resolve_width(None, Some("invalid")), 80);
    assert_eq!(resolve_width(None, Some("0")), 80);
    assert_eq!(resolve_width(Some(120), Some("40")), 120);
    assert_eq!(resolve_width(Some(0), Some("40")), 40);
    assert_eq!(resolve_width(None, Some("1")), 2);
    assert_eq!(resolve_width(None, Some("5000")), 1000);
  }

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
      "resource/  route  blue  ".to_owned(),
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

  #[test]
  fn detail_wrapping_handles_long_unbroken_and_combining_sequences() {
    let combining = format!("e{}", "\u{301}".repeat(32_768));
    for text in [
      "x".repeat(65_536),
      combining.clone(),
      format!("prefix/{combining}/{}", "z".repeat(256)),
    ] {
      let output = wrap_lines(&text, 20, 2);
      assert!(output.lines().all(|line| measure_text_width(line) <= 20));
      assert_eq!(
        output
          .lines()
          .map(|line| line.strip_prefix("  ").unwrap())
          .collect::<String>(),
        text,
      );
      if text.contains('\u{301}') {
        assert!(output.contains(&combining));
      }
    }
  }

  #[test]
  fn detail_wrapping_preserves_emoji_clusters_at_two_columns() {
    let clusters = ["👩‍💻", "🇫🇷", "1\u{fe0f}\u{20e3}", "👩🏽‍🚀", "e\u{301}"];
    let text = clusters.join("");
    let output = wrap_lines(&text, 2, 0);
    assert_eq!(output.lines().collect::<Vec<_>>(), clusters);
    assert!(output.lines().all(|line| measure_text_width(line) <= 2));
    assert_eq!(output.lines().collect::<String>(), text);
  }

  #[test]
  fn detail_wrapping_bounds_oversized_indic_graphemes() {
    use unicode_segmentation::UnicodeSegmentation;

    for count in [32, 4096] {
      let text = "क्".repeat(count);
      assert_eq!(text.graphemes(true).count(), 1);
      assert_eq!(measure_text_width(&text), count);
      for width in [8, 20, 40] {
        let output = wrap_lines(&text, width, 2);
        assert!(output.lines().all(|line| measure_text_width(line) <= width));
        assert_eq!(
          output
            .lines()
            .map(|line| line.strip_prefix("  ").unwrap())
            .collect::<String>(),
          text,
        );
      }
    }
  }

  #[test]
  fn detail_wrapping_retains_full_string_width_for_fitting_ligatures() {
    // Lam-Alef spans two graphemes but occupies one display column.
    let text = "لا".repeat(2);
    assert_eq!(measure_text_width(&text), 2);
    assert_eq!(wrap_lines(&text, 2, 0), text);
  }
}
