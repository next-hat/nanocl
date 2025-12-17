use std::{fs::OpenOptions, io::Write, os::unix::net::UnixDatagram, sync::Arc};

use nanocld_client::stubs::metric::MetricPartial;
use ntex::{http, rt};

use crate::models::SystemStateRef;

/// Build a display string for HTTP metric note.
fn build_display(data: &serde_json::Value) -> String {
  let status_num = data
    .get("status")
    .and_then(|v| v.as_i64())
    .unwrap_or_default() as u16;
  let status = http::StatusCode::from_u16(status_num)
    .map(|s| s.to_string())
    .unwrap_or_else(|_| status_num.to_string());
  let server_protocol = data
    .get("server_protocol")
    .and_then(|v| v.as_str())
    .unwrap_or("-");
  let method = data
    .get("request_method")
    .and_then(|v| v.as_str())
    .unwrap_or("-");
  let host = data.get("host").and_then(|v| v.as_str()).unwrap_or("");
  let uri = data.get("uri").and_then(|v| v.as_str()).unwrap_or("");

  // Extract path from URI (strip protocol://host if present)
  let path = if let Some(pos) = uri.find("://") {
    // URI contains protocol, find the path after the host
    uri
      .get(pos + 3..)
      .and_then(|s| s.find('/').map(|i| &s[i..]))
      .unwrap_or("/")
  } else {
    uri
  };

  let upstream_addr = data
    .get("upstream_addr")
    .and_then(|v| v.as_str())
    .unwrap_or("<none>");
  format!(
    "[{status}] {server_protocol} {method} {host}{path} -> {upstream_addr}"
  )
}

/// Build a display string for stream metric note.
fn build_stream_display(data: &serde_json::Value) -> String {
  let frontend = data.get("frontend").and_then(|v| v.as_str()).unwrap_or("-");
  let backend = data.get("backend").and_then(|v| v.as_str()).unwrap_or("-");
  let remote_addr = data
    .get("remote_addr")
    .and_then(|v| v.as_str())
    .unwrap_or("-");
  let upstream_addr = data
    .get("upstream_addr")
    .and_then(|v| v.as_str())
    .unwrap_or("<none>");
  let bytes_read = data
    .get("bytes_read")
    .and_then(|v| v.as_i64())
    .unwrap_or_default();
  let bytes_sent = data
    .get("bytes_sent")
    .and_then(|v| v.as_i64())
    .unwrap_or_default();

  // Detect if this is HTTPS based on frontend name (https_443) or bytes being 0 during SSL handshake
  let protocol = if frontend.starts_with("https_") || frontend.contains("ssl") {
    "HTTPS"
  } else {
    "TCP"
  };

  format!(
    "[{protocol}] {remote_addr} -> {frontend}/{backend} ({bytes_read}↓/{bytes_sent}↑) -> {upstream_addr}"
  )
}

/// Spawn a lightweight syslog receiver on a UNIX datagram socket,
/// parse each log line, POST the metric to nanocld, and write to http.log.
pub(crate) fn spawn(state: &SystemStateRef) {
  let socket_path = format!("{}/log/haproxy.sock", state.store.dir);
  let http_log_path = format!("{}/log/http.log", state.store.dir);
  let stream_socket_path =
    format!("{}/log/haproxy_stream.sock", state.store.dir);
  let stream_log_path = format!("{}/log/stream.log", state.store.dir);
  let state = Arc::clone(state);

  // Ensure log dir exists
  if let Err(e) = std::fs::create_dir_all(format!("{}/log", state.store.dir)) {
    log::warn!("logger: failed to create log dir: {e}");
    return;
  }

  // Clean up any pre-existing sockets
  let _ = std::fs::remove_file(&socket_path);
  let _ = std::fs::remove_file(&stream_socket_path);

  // Bind datagram socket for HAProxy httplog (local0)
  let sock = match UnixDatagram::bind(&socket_path) {
    Ok(s) => s,
    Err(e) => {
      log::warn!("logger: failed to bind {}: {e}", socket_path);
      return;
    }
  };

  // Bind datagram socket for HAProxy tcplog (local1)
  let stream_sock = match UnixDatagram::bind(&stream_socket_path) {
    Ok(s) => s,
    Err(e) => {
      log::warn!("logger: failed to bind {}: {e}", stream_socket_path);
      return;
    }
  };

  // Make sockets world-writable to avoid user/group mismatch inside container
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    for path in [&socket_path, &stream_socket_path] {
      if let Ok(metadata) = std::fs::metadata(path) {
        let mut perms = metadata.permissions();
        perms.set_mode(0o666);
        let _ = std::fs::set_permissions(path, perms);
      }
    }
  }

  // Spawn HTTP log receiver (local0)
  let state_http = Arc::clone(&state);
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      let mut http_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&http_log_path)
      {
        Ok(f) => f,
        Err(e) => {
          log::warn!("logger: failed to open {}: {e}", http_log_path);
          return;
        }
      };
      log::info!(
        "logger: receiving http logs on {} -> nanocld + {}",
        socket_path,
        http_log_path
      );
      let mut buf = vec![0u8; 65535];
      loop {
        match sock.recv(&mut buf) {
          Ok(sz) => {
            if sz == 0 {
              continue;
            }
            let line = match std::str::from_utf8(&buf[..sz]) {
              Ok(s) => s,
              Err(_) => continue,
            };
            let cleaned = if let Some(end) = line
              .strip_prefix('<')
              .and_then(|s| s.find('>'))
              .map(|i| i + 1)
            {
              &line[end..]
            } else {
              line
            };
            let cleaned = cleaned.trim_end();
            // Write raw JSON line to http.log
            if let Err(e) = writeln!(http_file, "{}", cleaned) {
              log::warn!("logger: failed to write http.log: {e}");
            }
            // Parse JSON directly from HAProxy log-format
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(cleaned)
            {
              let display = build_display(&data);
              let metric = MetricPartial {
                kind: "ncproxy.io/http".to_owned(),
                data,
                note: Some(display),
              };
              if let Err(e) = state_http.client.create_metric(&metric).await {
                log::warn!("logger: failed to POST http metric: {e}");
              }
            }
            // Silently ignore non-JSON lines (HAProxy error messages, etc.)
          }
          Err(e) => {
            log::warn!("logger: http recv error: {e}");
            ntex::time::sleep(std::time::Duration::from_millis(50)).await;
          }
        }
      }
    });
  });

  // Spawn stream log receiver (local1)
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      let mut stream_file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stream_log_path)
      {
        Ok(f) => f,
        Err(e) => {
          log::warn!("logger: failed to open {}: {e}", stream_log_path);
          return;
        }
      };
      log::info!(
        "logger: receiving stream logs on {} -> nanocld + {}",
        stream_socket_path,
        stream_log_path
      );
      let mut buf = vec![0u8; 65535];
      loop {
        match stream_sock.recv(&mut buf) {
          Ok(sz) => {
            if sz == 0 {
              continue;
            }
            let line = match std::str::from_utf8(&buf[..sz]) {
              Ok(s) => s,
              Err(_) => continue,
            };
            let cleaned = if let Some(end) = line
              .strip_prefix('<')
              .and_then(|s| s.find('>'))
              .map(|i| i + 1)
            {
              &line[end..]
            } else {
              line
            };
            let cleaned = cleaned.trim_end();
            // Write raw JSON line to stream.log
            if let Err(e) = writeln!(stream_file, "{}", cleaned) {
              log::warn!("logger: failed to write stream.log: {e}");
            }
            // Parse JSON directly from HAProxy log-format
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(cleaned)
            {
              let display = build_stream_display(&data);
              let metric = MetricPartial {
                kind: "ncproxy.io/stream".to_owned(),
                data,
                note: Some(display),
              };
              if let Err(e) = state.client.create_metric(&metric).await {
                log::warn!("logger: failed to POST stream metric: {e}");
              }
            } else {
              log::warn!("Internal haproxy error: {cleaned}");
            }
          }
          Err(e) => {
            log::warn!("logger: stream recv error: {e}");
            ntex::time::sleep(std::time::Duration::from_millis(50)).await;
          }
        }
      }
    });
  });
}
