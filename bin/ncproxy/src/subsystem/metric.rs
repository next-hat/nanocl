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
  stubs::metric::{HttpMetric, MetricPartial},
  NanocldClient,
};

use crate::models::SystemStateRef;

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
  let data = serde_json::from_str(&last_line).map_err(|e| {
    e.map_err_context(|| format!("metric failed to parse {last_line}"))
  })?;
  Ok(data)
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
      let data = serde_json::from_value::<HttpMetric>(data.clone())?;
      let upstream_addr = data.upstream_addr.unwrap_or("<none>".to_owned());
      let status = match http::StatusCode::from_u16(data.status as u16) {
        Err(_) => data.status.to_string(),
        Ok(status) => format!("{status}"),
      };
      let display = format!(
        "[{status}] {} {} {}{} -> {upstream_addr}",
        data.server_protocol, data.request_method, data.host, data.uri,
      );
      Some(display)
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
