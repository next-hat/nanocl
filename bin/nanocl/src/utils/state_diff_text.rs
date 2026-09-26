use std::io::Write;

use colored::Colorize;
use nanocl_error::io::{IoError, IoResult};
use serde_json::Value;
use similar::TextDiff;

/// Render safe configuration snapshots as a contextual YAML diff. The snapshots
/// have already had sensitive fields removed before reaching this function.
pub(crate) fn write_unified(
  writer: &mut impl Write,
  label: &str,
  before: Option<&Value>,
  after: Option<&Value>,
) -> IoResult<()> {
  if before == after {
    return Ok(());
  }
  let before_text = yaml_snapshot(before)?;
  let after_text = yaml_snapshot(after)?;
  let label = escape_controls(label, false);
  let before_label = if before.is_some() {
    &label
  } else {
    "/dev/null"
  };
  let after_label = if after.is_some() { &label } else { "/dev/null" };
  let diff = TextDiff::from_lines(&before_text, &after_text);
  let unified = diff
    .unified_diff()
    .context_radius(3)
    .header(before_label, after_label)
    .to_string();
  for line in unified.lines() {
    if line.starts_with("--- ") || line.starts_with("+++ ") {
      writeln!(writer, "{}", line.bold())?;
    } else if line.starts_with("@@ ") {
      writeln!(writer, "{}", line.cyan())?;
    } else if line.starts_with('-') {
      writeln!(writer, "{}", line.red())?;
    } else if line.starts_with('+') {
      writeln!(writer, "{}", line.green())?;
    } else {
      writeln!(writer, "{line}")?;
    }
  }
  Ok(())
}

fn yaml_snapshot(value: Option<&Value>) -> IoResult<String> {
  match value {
    Some(value) => Ok(escape_controls(
      &serde_yaml::to_string(value).map_err(|_| {
        IoError::invalid_data("State diff", "unable to render configuration")
      })?,
      true,
    )),
    None => Ok(String::new()),
  }
}

fn escape_controls(value: &str, preserve_newlines: bool) -> String {
  value
    .chars()
    .map(|ch| {
      if (ch.is_control() && !(preserve_newlines && ch == '\n'))
        || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
      {
        ch.escape_debug().to_string()
      } else {
        ch.to_string()
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  fn render(
    label: &str,
    before: Option<&Value>,
    after: Option<&Value>,
  ) -> String {
    let mut bytes = Vec::new();
    write_unified(&mut bytes, label, before, after).unwrap();
    let output = String::from_utf8(bytes).unwrap();
    // Make exact output assertions independent of the caller's color settings.
    regex::Regex::new("\u{1b}\\[[0-9;]*m")
      .unwrap()
      .replace_all(&output, "")
      .into_owned()
  }

  #[test]
  fn nested_port_change_has_yaml_context() {
    let before = json!({
      "Name": "deploy-example.com",
      "Kind": "ProxyRule",
      "Data": {
        "Rules": [{
          "Domain": "deploy-example.com", "Network": "global", "Port": 9000
        }]
      }
    });
    let mut after = before.clone();
    after["Data"]["Rules"][0]["Port"] = json!(9001);
    assert_eq!(
      render("resource/deploy-example.com", Some(&before), Some(&after)),
      concat!(
        "--- resource/deploy-example.com\n",
        "+++ resource/deploy-example.com\n",
        "@@ -2,6 +2,6 @@\n",
        "   Rules:\n",
        "   - Domain: deploy-example.com\n",
        "     Network: global\n",
        "-    Port: 9000\n",
        "+    Port: 9001\n",
        " Kind: ProxyRule\n",
        " Name: deploy-example.com\n",
      )
    );
  }

  #[test]
  fn distant_changes_have_separate_hunks() {
    let before = json!({
      "A": 0, "B": 1, "C": 2, "D": 3, "E": 4, "F": 5,
      "G": 6, "H": 7, "I": 8, "J": 9, "K": 10, "L": 11,
    });
    let mut after = before.clone();
    after["B"] = json!(101);
    after["K"] = json!(110);
    assert_eq!(
      render("resource/example", Some(&before), Some(&after)),
      concat!(
        "--- resource/example\n",
        "+++ resource/example\n",
        "@@ -1,5 +1,5 @@\n",
        " A: 0\n",
        "-B: 1\n",
        "+B: 101\n",
        " C: 2\n",
        " D: 3\n",
        " E: 4\n",
        "@@ -8,5 +8,5 @@\n",
        " H: 7\n",
        " I: 8\n",
        " J: 9\n",
        "-K: 10\n",
        "+K: 110\n",
        " L: 11\n",
      )
    );
  }

  #[test]
  fn create_and_remove_use_null_side() {
    let value = json!({ "Name": "example" });
    assert_eq!(
      render("namespace/example", None, Some(&value)),
      concat!(
        "--- /dev/null\n",
        "+++ namespace/example\n",
        "@@ -0,0 +1 @@\n",
        "+Name: example\n",
      )
    );
    assert_eq!(
      render("namespace/example", Some(&value), None),
      concat!(
        "--- namespace/example\n",
        "+++ /dev/null\n",
        "@@ -1 +0,0 @@\n",
        "-Name: example\n",
      )
    );
  }

  #[test]
  fn equal_documents_have_no_output() {
    let value = json!({ "Name": "example", "Data": { "Port": 9000 } });
    assert_eq!(render("resource/example", Some(&value), Some(&value)), "");
    assert_eq!(render("resource/example", None, None), "");
  }

  #[test]
  fn control_characters_cannot_inject_terminal_output() {
    let value = json!({ "Name": "one\ntwo", "Data": "\u{1b}[2J\u{202e}" });
    let output = render("resource/a\n\u{1b}[2J\u{202e}", None, Some(&value));
    assert!(output.starts_with(concat!(
      "--- /dev/null\n",
      "+++ resource/a\\n\\u{1b}[2J\\u{202e}\n",
    )));
    assert!(!output.contains('\u{1b}'));
    assert!(!output.contains('\u{202e}'));
    assert!(output.contains("+Name: |-\n+  one\n+  two\n"));
    assert!(output.contains("@@ -0,0 +1,4 @@\n"));
  }
}
