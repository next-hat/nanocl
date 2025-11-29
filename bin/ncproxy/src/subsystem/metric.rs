/// Watch for change inside the access log directory
/// and print them to the console
/// that way we can see what is happening in real time with docker logs
/// so the daemon will be able to save them to the database
/// and broadcast them in real time
use std::{
  fs::File,
  io::{BufReader, Read, Seek, SeekFrom},
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

use crate::models::SystemStateRef;

/// Fallback parser for HAProxy `option httplog` raw lines.
/// Returns a JSON representation with a subset of fields required by HttpMetric.
fn parse_haproxy_httplog(line: &str) -> Option<serde_json::Value> {
  // Skip possible syslog prefix ending with "]: "
  let start = line.find("]: ").map(|i| i + 3).unwrap_or(0);
  let raw = &line[start..];
  // Find request line in quotes
  let req_start = raw.find('"')?;
  let req_end = raw.rfind('"')?;
  if req_end <= req_start {
    return None;
  }
  let request_line = &raw[req_start + 1..req_end];
  let (method, uri, proto) = {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let uri = parts.next().unwrap_or("");
    let proto = parts.next().unwrap_or("");
    (method, uri, proto)
  };
  // Portion before request line (tokens)
  let before_req = raw[..req_start].trim();
  let tokens: Vec<&str> = before_req.split_whitespace().collect();
  if tokens.len() < 7 {
    return None;
  }
  // tokens mapping
  let ip_port = tokens[0];
  let client_ip = ip_port.split(':').next().unwrap_or(ip_port);
  let datetime = tokens[1].trim_matches('[').trim_matches(']');
  let frontend = tokens[2];
  let backend_server = tokens[3];
  let mut bs_iter = backend_server.split('/');
  let backend = bs_iter.next().unwrap_or("");
  let server = bs_iter.next().unwrap_or("");
  let timings = tokens[4];
  let mut t_iter = timings.split('/');
  let tq = t_iter.next().unwrap_or("0");
  let tw = t_iter.next().unwrap_or("0");
  let tc = t_iter.next().unwrap_or("0");
  let tr = t_iter.next().unwrap_or("0");
  let tt = t_iter.next().unwrap_or("0");
  let status = tokens[5];
  let bytes = tokens[6];
  // Build JSON value
  let mut obj = serde_json::Map::new();
  obj.insert("remote_addr".into(), client_ip.into());
  obj.insert("time_local".into(), datetime.into());
  obj.insert("request_method".into(), method.into());
  obj.insert("uri".into(), uri.into());
  obj.insert("server_protocol".into(), proto.into());
  obj.insert("status".into(), status.parse::<i64>().unwrap_or(0).into());
  obj.insert(
    "body_bytes_sent".into(),
    bytes.parse::<i64>().unwrap_or(0).into(),
  );
  obj.insert("frontend".into(), frontend.into());
  obj.insert("backend".into(), backend.into());
  obj.insert("server".into(), server.into());
  obj.insert("tq".into(), tq.parse::<i64>().unwrap_or(0).into());
  obj.insert("tw".into(), tw.parse::<i64>().unwrap_or(0).into());
  obj.insert("tc".into(), tc.parse::<i64>().unwrap_or(0).into());
  obj.insert("tr".into(), tr.parse::<i64>().unwrap_or(0).into());
  obj.insert("tt".into(), tt.parse::<i64>().unwrap_or(0).into());
  // Maintain compatibility with existing HttpMetric fields
  obj.insert("host".into(), serde_json::Value::Null); // Host not present unless captured separately
  obj.insert("upstream_addr".into(), serde_json::Value::Null);
  obj.insert("request_time".into(), tt.parse::<i64>().unwrap_or(0).into());
  Some(serde_json::Value::Object(obj))
}

fn read(path: &Path) -> IoResult<serde_json::Value> {
  if !path.exists() {
    return Err(IoError::not_found("Metric", &format!("{}", path.display())));
  }
  let file = match File::open(path) {
    Ok(file) => file,
    Err(e) => {
      return Err(e.map_err_context(|| "metric").into());
    }
  };
  let mut buf_reader = BufReader::new(file);
  let mut pos = match buf_reader.seek(SeekFrom::End(-2)) {
    Ok(pos) => pos,
    Err(e) => {
      return Err(e.map_err_context(|| "metric").into());
    }
  };
  let mut last_line = String::new();
  while pos > 0 {
    match buf_reader.seek(SeekFrom::Start(pos)) {
      Ok(_) => {}
      Err(e) => {
        log::warn!("metric::read: {e}");
        return Err(e.map_err_context(|| "metric").into());
      }
    }
    let mut buffer = [0; 1];
    let res = buf_reader.read_exact(&mut buffer);
    if buffer[0] == b'\n' || res.is_err() {
      break;
    }
    last_line.insert(0, buffer[0] as char);
    pos -= 1;
  }
  log::trace!("metric::read: {last_line}");
  match serde_json::from_str(&last_line) {
    Ok(v) => Ok(v),
    Err(_json_err) => {
      if let Some(v) = parse_haproxy_httplog(&last_line) {
        Ok(v)
      } else {
        let msg = format!("failed to parse {last_line}");
        Err(IoError::invalid_data("metric", &msg))
      }
    }
  }
}

async fn create(
  kind: &str,
  path: &Path,
  client: &NanocldClient,
) -> IoResult<()> {
  let data = read(path)?;
  log::trace!("metric::create: {kind} {data}");
  let display = match kind {
    "ncproxy.io/http" => {
      // Prefer strict struct when compatible JSON is present
      if let Ok(parsed) = serde_json::from_value::<HttpMetric>(data.clone()) {
        let upstream_addr = parsed.upstream_addr.unwrap_or("<none>".to_owned());
        let status = match http::StatusCode::from_u16(parsed.status as u16) {
          Err(_) => parsed.status.to_string(),
          Ok(status) => format!("{status}"),
        };
        Some(format!(
          "[{status}] {} {} {}{} -> {upstream_addr}",
          parsed.server_protocol, parsed.request_method, parsed.host, parsed.uri,
        ))
      } else {
        // Fallback for raw httplog-derived JSON
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
        let upstream_addr = data
          .get("upstream_addr")
          .and_then(|v| v.as_str())
          .unwrap_or("<none>");
        Some(format!(
          "[{status}] {server_protocol} {method} {host}{uri} -> {upstream_addr}"
        ))
      }
    }
    "ncproxy.io/stream" => None,
    _ => None,
  };
  let metric = MetricPartial {
    kind: kind.to_owned(),
    data,
    note: display,
  };
  client.create_metric(&metric).await?;
  Ok(())
}

async fn watch(state: &SystemStateRef) -> IoResult<()> {
  let path = format!("{}/log", state.store.dir);
  let path = Path::new(&path);
  if !path.exists() {
    std::fs::create_dir_all(path)
      .map_err(|e| e.map_err_context(|| "metric"))?;
  }
  let (tx, rx) = std::sync::mpsc::channel();
  // Automatically select the best implementation for your platform.
  // You can also access each implementation directly e.g. INotifyWatcher.
  let mut watcher = match RecommendedWatcher::new(
    tx,
    Config::default()
      .with_compare_contents(true)
      .with_poll_interval(Duration::from_secs(2)),
  ) {
    Ok(watcher) => watcher,
    Err(e) => {
      return Err(IoError::interrupted("metric", &e.to_string()));
    }
  };
  // Add a path to be watched. All files and directories at that path and
  // below will be monitored for changes.
  watcher.watch(path, RecursiveMode::Recursive).unwrap();
  log::debug!("metric::watch: {}", path.display());
  for res in rx {
    let event = match res {
      Ok(event) => event,
      Err(e) => {
        log::warn!("metric::watch: {e}");
        continue;
      }
    };
    log::trace!("metric::watch: {event:?}");
    if let notify::EventKind::Modify(notify::event::ModifyKind::Data(_)) =
      event.kind
    {
      let path = event.paths.first();
      if let Some(path) = path {
        let file_name = path
          .file_name()
          .unwrap_or_default()
          .to_str()
          .unwrap_or_default();
        let kind = match file_name {
          "http.log" => "ncproxy.io/http",
          "stream.log" => "ncproxy.io/stream",
          _ => {
            continue;
          }
        };
        if let Err(err) = create(kind, path, &state.client).await {
          log::warn!("metric::watch: {err}");
        }
      }
    }
  }
  Ok(())
}

/// Spawn new thread and watch for change inside the access log directory
/// to print them to the console that way we can see what is happening in real time with docker logs
/// and the nanocl daemon will be able to save them to the database
pub(crate) fn spawn(state: &SystemStateRef) {
  let state = Arc::clone(state);
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      if let Err(err) = watch(&state).await {
        log::warn!("metric::spawn: {err}");
      }
      rt::Arbiter::current().stop();
    });
  });
}
