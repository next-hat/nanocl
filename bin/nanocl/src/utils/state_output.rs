use std::io::{self, Write};

use nanocl_error::io::IoResult;

use crate::models::{StateOutput, StateOutputEvent, StateOutputRecord};

impl StateOutput {
  pub(crate) fn emit(&self, event: StateOutputEvent<'_>) -> IoResult<()> {
    self.write_to(&mut io::stdout().lock(), event)
  }

  pub(crate) fn write_to(
    &self,
    writer: &mut impl Write,
    event: StateOutputEvent<'_>,
  ) -> IoResult<()> {
    let record = StateOutputRecord {
      schema_version: 1,
      operation: self.operation,
      statefile: self.statefile.as_deref(),
      event,
    };
    let mut line = serde_json::to_vec(&record)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};

  #[derive(Default)]
  struct RecordWriter {
    bytes: Vec<u8>,
    flushes: usize,
  }

  impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
      self.bytes.extend_from_slice(bytes);
      Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
      self.flushes += 1;
      Ok(())
    }
  }

  struct FailingWriter {
    fail_on_flush: bool,
  }

  impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
      if self.fail_on_flush {
        Ok(bytes.len())
      } else {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
      }
    }

    fn flush(&mut self) -> io::Result<()> {
      Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
    }
  }

  #[test]
  fn writes_separate_escaped_records_and_flushes_each() {
    let output = StateOutput {
      operation: "apply",
      statefile: Some("state\n\"file\".yml".to_owned()),
    };
    let mut writer = RecordWriter::default();
    output
      .write_to(
        &mut writer,
        StateOutputEvent::State {
          status: "started",
          completed: 0,
          total: 2,
          success: None,
          elapsed_ms: 0,
        },
      )
      .unwrap();
    assert_eq!(writer.flushes, 1);
    output
      .write_to(
        &mut writer,
        StateOutputEvent::Item {
          resource: "cargo/demo",
          status: "failed",
          completed: 1,
          total: 2,
          success: Some(false),
          elapsed_ms: 12,
          error: Some("first line\n\"second line\""),
        },
      )
      .unwrap();
    assert_eq!(writer.flushes, 2);
    output
      .write_to(
        &mut writer,
        StateOutputEvent::Result {
          success: false,
          error: Some("first line\n\"second line\""),
        },
      )
      .unwrap();
    assert_eq!(writer.flushes, 3);

    let text = String::from_utf8(writer.bytes).unwrap();
    assert!(text.ends_with('\n'));
    let records: Vec<Value> = text
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(records.len(), 3);
    for record in &records {
      assert_eq!(record["schema_version"], 1);
      assert_eq!(record["operation"], "apply");
      assert_eq!(record["statefile"], "state\n\"file\".yml");
      assert!(record.get("event").is_none());
    }
    assert_eq!(records[0]["type"], "state");
    assert_eq!(records[0]["completed"], 0);
    assert_eq!(records[0]["total"], 2);
    assert!(records[0].get("success").is_none());
    assert_eq!(records[1]["type"], "item");
    assert_eq!(records[1]["resource"], "cargo/demo");
    assert_eq!(records[1]["status"], "failed");
    assert_eq!(records[1]["completed"], 1);
    assert_eq!(records[1]["elapsed_ms"], 12);
    assert_eq!(records[1]["success"], false);
    assert_eq!(records[1]["error"], "first line\n\"second line\"");
    assert_eq!(records[2]["type"], "result");
    assert_eq!(records[2]["success"], false);
    assert_eq!(records[2]["error"], "first line\n\"second line\"");
  }

  #[test]
  fn writes_image_fields_and_omits_absent_values() {
    let output = StateOutput {
      operation: "apply",
      statefile: None,
    };
    let mut bytes = Vec::new();
    output
      .write_to(
        &mut bytes,
        StateOutputEvent::Image {
          resource: "cargo/demo",
          image: "alpine:latest",
          node: "node-a",
          layer: Some("sha256:layer"),
          status: "Downloading",
          current: Some(4),
          total: Some(10),
          error: None,
        },
      )
      .unwrap();
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
      record,
      json!({
        "schema_version": 1,
        "operation": "apply",
        "type": "image",
        "resource": "cargo/demo",
        "image": "alpine:latest",
        "node": "node-a",
        "layer": "sha256:layer",
        "status": "Downloading",
        "current": 4,
        "total": 10,
      })
    );
    bytes.clear();
    output
      .write_to(
        &mut bytes,
        StateOutputEvent::Image {
          resource: "cargo/demo",
          image: "alpine:latest",
          node: "node-a",
          layer: None,
          status: "failed",
          current: None,
          total: None,
          error: Some("download failed"),
        },
      )
      .unwrap();
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
      record,
      json!({
        "schema_version": 1,
        "operation": "apply",
        "type": "image",
        "resource": "cargo/demo",
        "image": "alpine:latest",
        "node": "node-a",
        "status": "failed",
        "error": "download failed",
      })
    );
    let output = StateOutput {
      operation: "remove",
      statefile: None,
    };
    bytes.clear();
    output
      .write_to(
        &mut bytes,
        StateOutputEvent::Result {
          success: true,
          error: None,
        },
      )
      .unwrap();
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
      record,
      json!({
        "schema_version": 1,
        "operation": "remove",
        "type": "result",
        "success": true,
      })
    );
  }

  #[test]
  fn propagates_write_and_flush_errors() {
    let output = StateOutput {
      operation: "apply",
      statefile: None,
    };
    for fail_on_flush in [false, true] {
      let error = output
        .write_to(
          &mut FailingWriter { fail_on_flush },
          StateOutputEvent::Result {
            success: true,
            error: None,
          },
        )
        .unwrap_err();
      assert_eq!(error.inner.kind(), io::ErrorKind::BrokenPipe);
    }
  }
}
