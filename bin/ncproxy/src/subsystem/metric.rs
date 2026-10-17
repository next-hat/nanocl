//! Forward complete access-log records to the daemon without logging payloads.
use std::{
  collections::HashMap,
  fs::File,
  io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
  os::unix::fs::MetadataExt,
  path::Path,
  sync::Arc,
  time::Duration,
};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ntex::{http, rt};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::metric::{HttpMetric, MetricPartial},
};

use crate::models::{MetricCursor, SystemStateRef};

const MAX_RECORD_BYTES: usize = 1024 * 1024;
const CHECKPOINT_BYTES: usize = 64;

/// Retain a small suffix to detect copy-truncation followed by rapid regrowth.
fn checkpoint(cursor: &mut MetricCursor, bytes: &[u8]) {
  if bytes.len() >= CHECKPOINT_BYTES {
    cursor.checkpoint.clear();
    cursor
      .checkpoint
      .extend_from_slice(&bytes[bytes.len() - CHECKPOINT_BYTES..]);
  } else {
    let excess =
      (cursor.checkpoint.len() + bytes.len()).saturating_sub(CHECKPOINT_BYTES);
    cursor.checkpoint.drain(..excess);
    cursor.checkpoint.extend_from_slice(bytes);
  }
}

/// Resume this file, resetting on replacement or truncation. At startup skip
/// existing records and discard any unfinished historical tail through its LF.
fn prepare_reader<R: Read + Seek>(
  reader: &mut R,
  cursor: &mut MetricCursor,
  identity: (u64, u64),
  skip_existing: bool,
) -> io::Result<u64> {
  let len = reader.seek(SeekFrom::End(0))?;
  if skip_existing {
    *cursor = MetricCursor::default();
    cursor.identity = Some(identity);
    cursor.offset = len;
    let tail_len = len.min(CHECKPOINT_BYTES as u64) as usize;
    reader.seek(SeekFrom::Start(len - tail_len as u64))?;
    cursor.checkpoint.resize(tail_len, 0);
    reader.read_exact(&mut cursor.checkpoint)?;
    cursor.discard = cursor.checkpoint.last().is_some_and(|b| *b != b'\n');
  } else {
    let mut reset = cursor.identity != Some(identity) || len < cursor.offset;
    if !reset && !cursor.checkpoint.is_empty() {
      reader.seek(SeekFrom::Start(
        cursor.offset - cursor.checkpoint.len() as u64,
      ))?;
      let mut actual = [0; CHECKPOINT_BYTES];
      let actual = &mut actual[..cursor.checkpoint.len()];
      reader.read_exact(actual)?;
      reset = actual != cursor.checkpoint.as_slice();
    }
    if reset {
      *cursor = MetricCursor::default();
      cursor.identity = Some(identity);
    }
  }
  reader.seek(SeekFrom::Start(cursor.offset))?;
  Ok(len)
}

/// Consume bytes once, preserving unfinished UTF-8/JSON across notifications.
/// Oversized records are discarded through their newline with bounded memory.
fn read_record<R: BufRead>(
  reader: &mut R,
  cursor: &mut MetricCursor,
) -> io::Result<Option<Vec<u8>>> {
  loop {
    let bytes = reader.fill_buf()?;
    if bytes.is_empty() {
      return Ok(None);
    }
    let newline = bytes.iter().position(|byte| *byte == b'\n');
    let consumed = newline.map_or(bytes.len(), |index| index + 1);
    let content_len = newline.unwrap_or(bytes.len());
    if !cursor.discard {
      if cursor.pending.len() + content_len > MAX_RECORD_BYTES {
        cursor.pending.clear();
        cursor.discard = true;
        log::warn!("metric::read: discarded oversized record");
      } else {
        cursor.pending.extend_from_slice(&bytes[..content_len]);
      }
    }
    checkpoint(cursor, &bytes[..consumed]);
    cursor.offset += consumed as u64;
    reader.consume(consumed);
    if newline.is_some() {
      if cursor.discard {
        cursor.discard = false;
      } else if !cursor.pending.is_empty() {
        return Ok(Some(std::mem::take(&mut cursor.pending)));
      }
    }
  }
}

fn parse_metric(kind: &str, record: &[u8]) -> IoResult<MetricPartial> {
  // Serde errors can contain an offending value; never include them or the
  // original record in diagnostic messages.
  let mut data: serde_json::Value = serde_json::from_slice(record)
    .map_err(|_| IoError::invalid_data("metric", "invalid JSON record"))?;
  let display = match kind {
    "ncproxy.io/http" => {
      let object = data.as_object_mut().ok_or_else(|| {
        IoError::invalid_data("metric", "invalid HTTP record")
      })?;
      // Also scrub records written before the Nginx logging policy changed.
      // Empty strings preserve HttpMetric's existing deserialization contract.
      for field in ["query_string", "request_body", "http_referrer"] {
        object
          .insert(field.to_owned(), serde_json::Value::String(String::new()));
      }
      let parsed = serde_json::from_value::<HttpMetric>(data.clone())
        .map_err(|_| IoError::invalid_data("metric", "invalid HTTP record"))?;
      let upstream_addr = parsed.upstream_addr.unwrap_or("<none>".to_owned());
      let status = match http::StatusCode::from_u16(parsed.status as u16) {
        Err(_) => parsed.status.to_string(),
        Ok(status) => format!("{status}"),
      };
      Some(format!(
        "[{status}] {} {} {}{} -> {upstream_addr}",
        parsed.server_protocol, parsed.request_method, parsed.host, parsed.uri,
      ))
    }
    _ => None,
  };
  Ok(MetricPartial {
    kind: kind.to_owned(),
    data,
    note: display,
  })
}

async fn create(
  kind: &str,
  path: &Path,
  cursor: &mut MetricCursor,
  client: &NanocldClient,
) -> IoResult<()> {
  let file = match File::open(path) {
    Ok(file) => file,
    Err(err) if err.kind() == io::ErrorKind::NotFound => {
      *cursor = MetricCursor::default();
      return Ok(());
    }
    Err(err) => return Err(err.map_err_context(|| "metric").into()),
  };
  let metadata = file
    .metadata()
    .map_err(|err| err.map_err_context(|| "metric"))?;
  let mut reader = BufReader::new(file);
  let end = prepare_reader(
    &mut reader,
    cursor,
    (metadata.dev(), metadata.ino()),
    false,
  )
  .map_err(|err| err.map_err_context(|| "metric"))?;
  // New appends have their own notifications. Yield at this snapshot's EOF so
  // a busy log cannot keep the other log's events waiting indefinitely.
  let mut reader = reader.take(end.saturating_sub(cursor.offset));
  while let Some(record) = read_record(&mut reader, cursor)
    .map_err(|err| err.map_err_context(|| "metric"))?
  {
    let metric = match parse_metric(kind, &record) {
      Ok(metric) => metric,
      Err(err) => {
        log::warn!("metric::create: {err}");
        continue;
      }
    };
    if client.create_metric(&metric).await.is_err() {
      log::warn!("metric::create: failed to forward {kind} metric");
    }
  }
  Ok(())
}

async fn watch(state: &SystemStateRef) -> IoResult<()> {
  let path = format!("{}/log", state.store.dir);
  let path = Path::new(&path);
  if !path.exists() {
    std::fs::create_dir_all(path)
      .map_err(|e| e.map_err_context(|| "metric"))?;
  }
  let path = std::fs::canonicalize(path)
    .map_err(|err| err.map_err_context(|| "metric"))?;
  let (tx, rx) = std::sync::mpsc::channel();
  let mut watcher = RecommendedWatcher::new(
    tx,
    Config::default()
      .with_compare_contents(true)
      .with_poll_interval(Duration::from_secs(2)),
  )
  .map_err(|err| IoError::interrupted("metric", &err.to_string()))?;
  watcher
    .watch(&path, RecursiveMode::NonRecursive)
    .map_err(|err| IoError::interrupted("metric", &err.to_string()))?;
  let mut cursors = HashMap::from([
    (path.join("http.log"), MetricCursor::default()),
    (path.join("stream.log"), MetricCursor::default()),
  ]);
  for (path, cursor) in &mut cursors {
    match File::open(path) {
      Ok(mut file) => {
        let metadata = file
          .metadata()
          .map_err(|err| err.map_err_context(|| "metric"))?;
        prepare_reader(
          &mut file,
          cursor,
          (metadata.dev(), metadata.ino()),
          true,
        )
        .map_err(|err| err.map_err_context(|| "metric"))?;
      }
      Err(err) if err.kind() == io::ErrorKind::NotFound => {}
      Err(err) => return Err(err.map_err_context(|| "metric").into()),
    }
  }
  log::debug!("metric::watch: {}", path.display());
  for res in rx {
    let event = match res {
      Ok(event) => event,
      Err(e) => {
        log::warn!("metric::watch: {e}");
        continue;
      }
    };
    if !matches!(
      event.kind,
      notify::EventKind::Create(_)
        | notify::EventKind::Remove(_)
        | notify::EventKind::Modify(
          notify::event::ModifyKind::Data(_)
            | notify::event::ModifyKind::Name(_)
            | notify::event::ModifyKind::Any
        )
    ) {
      continue;
    }
    for path in event.paths {
      let Some(cursor) = cursors.get_mut(&path) else {
        continue;
      };
      let kind = if path.file_name().is_some_and(|name| name == "http.log") {
        "ncproxy.io/http"
      } else {
        "ncproxy.io/stream"
      };
      if let Err(err) = create(kind, &path, cursor, &state.client).await {
        log::warn!("metric::watch: {err}");
      }
    }
  }
  Ok(())
}

/// Spawn the access-log watcher and forward complete metrics to the daemon.
pub(crate) fn spawn(state: &SystemStateRef) {
  let state = Arc::clone(state);
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      if let Err(err) = watch(&state).await {
        log::warn!("metric::spawn: {err}");
      }
      rt::Arbiter::current().stop();
    });
  });
}

#[cfg(test)]
mod tests {
  use std::io::{BufReader, Cursor};

  use super::*;

  fn read_available(
    bytes: &[u8],
    state: &mut MetricCursor,
    identity: (u64, u64),
  ) -> Vec<Vec<u8>> {
    let mut reader = BufReader::with_capacity(3, Cursor::new(bytes));
    prepare_reader(&mut reader, state, identity, false).unwrap();
    let mut records = Vec::new();
    while let Some(record) = read_record(&mut reader, state).unwrap() {
      records.push(record);
    }
    records
  }

  #[test]
  fn frames_partial_utf8_batches_without_duplicates() {
    let mut state = MetricCursor::default();
    assert!(read_available(b"", &mut state, (1, 1)).is_empty());
    assert!(read_available(b"\n", &mut state, (1, 1)).is_empty());
    let first = "{\"uri\":\"/é\"}".as_bytes();
    let split = first.iter().position(|byte| *byte == 0xc3).unwrap() + 1;
    assert!(read_available(&first[..split], &mut state, (1, 1)).is_empty());
    assert!(read_available(first, &mut state, (1, 1)).is_empty());
    let mut complete = first.to_vec();
    complete.extend_from_slice(b"\n{}\n{\"later\":");
    assert_eq!(
      read_available(&complete, &mut state, (1, 1)),
      vec![first.to_vec(), b"{}".to_vec()]
    );
    assert!(read_available(&complete, &mut state, (1, 1)).is_empty());
    complete.extend_from_slice(b"true}\n");
    assert_eq!(
      read_available(&complete, &mut state, (1, 1)),
      vec![b"{\"later\":true}".to_vec()]
    );
    let mut state = MetricCursor::default();
    assert!(read_available(b"0", &mut state, (1, 1)).is_empty());
    assert_eq!(
      read_available(b"0\n", &mut state, (1, 1)),
      vec![b"0".to_vec()]
    );
  }

  #[test]
  fn resumes_after_rotation_truncation_and_regrowth() {
    let mut state = MetricCursor::default();
    assert!(read_available(b"unfinished", &mut state, (1, 1)).is_empty());
    assert_eq!(
      read_available(b"{}\n", &mut state, (1, 2)),
      vec![b"{}".to_vec()]
    );
    assert_eq!(
      read_available(b"0\n", &mut state, (1, 2)),
      vec![b"0".to_vec()]
    );
    assert_eq!(
      read_available(b"1\n2\n", &mut state, (1, 2)),
      vec![b"1".to_vec(), b"2".to_vec()]
    );
  }

  #[test]
  fn skips_existing_records_and_unfinished_startup_tail() {
    let mut state = MetricCursor::default();
    let mut existing = Cursor::new(b"{}\npartial".to_vec());
    prepare_reader(&mut existing, &mut state, (1, 1), true).unwrap();
    assert_eq!(
      read_available(b"{}\npartial-rest\ntrue\n", &mut state, (1, 1)),
      vec![b"true".to_vec()]
    );
    prepare_reader(&mut Cursor::new(b"{}\n"), &mut state, (1, 2), true)
      .unwrap();
    assert_eq!(
      read_available(b"{}\nfalse\n", &mut state, (1, 2)),
      vec![b"false".to_vec()]
    );
  }

  #[test]
  fn snapshot_end_defers_new_appends_until_next_notification() {
    let mut state = MetricCursor::default();
    let mut reader = BufReader::new(Cursor::new(b"{}\npar".to_vec()));
    let end = prepare_reader(&mut reader, &mut state, (1, 1), false).unwrap();
    reader
      .get_mut()
      .get_mut()
      .extend_from_slice(b"tial\ntrue\n");
    let mut limited = reader.take(end.saturating_sub(state.offset));
    assert_eq!(
      read_record(&mut limited, &mut state).unwrap(),
      Some(b"{}".to_vec())
    );
    assert!(read_record(&mut limited, &mut state).unwrap().is_none());
    assert_eq!(state.offset, end);
    assert_eq!(state.pending, b"par");
    assert_eq!(
      read_available(b"{}\npartial\ntrue\n", &mut state, (1, 1)),
      vec![b"partial".to_vec(), b"true".to_vec()]
    );
  }

  #[test]
  fn bounds_unfinished_records_and_recovers_at_newline() {
    let mut state = MetricCursor::default();
    let mut bytes = vec![b'x'; MAX_RECORD_BYTES + 1];
    assert!(read_available(&bytes, &mut state, (1, 1)).is_empty());
    assert!(state.pending.is_empty());
    assert!(state.discard);
    assert!(state.checkpoint.len() <= CHECKPOINT_BYTES);
    bytes.extend_from_slice(b"tail\n{}\n");
    assert_eq!(
      read_available(&bytes, &mut state, (1, 1)),
      vec![b"{}".to_vec()]
    );
  }

  #[test]
  fn malformed_record_does_not_block_next_record_or_expose_values() {
    let mut state = MetricCursor::default();
    let records =
      read_available(b"{\"secret\":secret-token}\n{}\n", &mut state, (1, 1));
    let err = parse_metric("ncproxy.io/stream", &records[0]).unwrap_err();
    assert!(!err.to_string().contains("secret"));
    assert!(parse_metric("ncproxy.io/stream", &records[1]).is_ok());
    let err =
      parse_metric("ncproxy.io/http", br#"{"date_gmt":"secret-token"}"#)
        .unwrap_err();
    assert!(!err.to_string().contains("secret"));
  }

  #[test]
  fn scrubs_legacy_http_credentials_before_forwarding() {
    let record = serde_json::json!({
      "date_gmt": "2026-10-17T00:00:00Z", "uri": "/é", "host": "example.test",
      "remote_addr": "127.0.0.1", "realip_remote_addr": "127.0.0.1",
      "server_protocol": "HTTP/1.1", "request_method": "POST",
      "bytes_sent": "100", "content_length": "3", "status": "200",
      "request_time": "0.1", "body_bytes_sent": "2", "proxy_host": "",
      "upstream_addr": "", "query_string": "key=secret-query",
      "request_body": "secret-body", "http_referrer": "https://example.test/?key=secret-referrer",
      "content_type": "application/json", "http_user_agent": "test",
      "http_accept_language": "en"
    });
    let metric =
      parse_metric("ncproxy.io/http", &serde_json::to_vec(&record).unwrap())
        .unwrap();
    for field in ["query_string", "request_body", "http_referrer"] {
      assert_eq!(metric.data[field], "");
    }
    assert!(!serde_json::to_string(&metric).unwrap().contains("secret-"));
    assert!(metric.note.unwrap().contains("example.test/é"));
  }
}
