/// Watch for change inside the access log directory
/// and print them to the console
/// that way we can see what is happening in real time with docker logs
/// so the daemon will be able to save them to the database
/// and broadcast them in real time
use std::{
  fs::File,
  path::Path,
  time::Duration,
  io::{Read, Seek, BufReader, SeekFrom},
};

use ntex::rt;
use notify::{Config, Watcher, RecursiveMode, RecommendedWatcher};

use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocld_client::{NanocldClient, stubs::metric::MetricPartial};

fn read(path: &Path) -> IoResult<(&'static str, serde_json::Value)> {
  if !path.exists() {
    return Err(IoError::not_found("Metric", &format!("{}", path.display())));
  }
  let file_name = path
    .file_name()
    .unwrap_or_default()
    .to_str()
    .unwrap_or_default();
  let kind = match file_name {
    "http.log" => "ncproxy.io/http",
    "stream.log" => "ncproxy.io/stream",
    _ => {
      log::warn!("metric::read: {}", file_name);
      return Err(IoError::invalid_data(
        "Metric",
        &format!("{}", path.display()),
      ));
    }
  };
  let file = match File::open(path) {
    Ok(file) => file,
    Err(e) => {
      log::warn!("metric::read: {e}");
      return Err(e.map_err_context(|| "metric").into());
    }
  };
  let mut buf_reader = BufReader::new(file);
  let mut pos = match buf_reader.seek(SeekFrom::End(-2)) {
    Ok(pos) => pos,
    Err(e) => {
      log::warn!("metric::read: {e}");
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
  let data = serde_json::from_str(&last_line)
    .map_err(|e| e.map_err_context(|| "metric"))?;
  Ok((kind, data))
}

async fn create(path: &Path, client: &NanocldClient) -> IoResult<()> {
  let (kind, data) = read(path)?;
  let metric = MetricPartial {
    kind: kind.to_owned(),
    data,
  };
  client.create_metric(&metric).await?;
  Ok(())
}

async fn watch(client: &NanocldClient) -> IoResult<()> {
  let path = Path::new("/var/log/nginx/access");
  if !path.exists() {
    return Err(IoError::not_found("Metric", &format!("{}", path.display())));
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
      log::warn!("metric::watch: {e}");
      return Err(IoError::interupted("metric", &e.to_string()));
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
      let path = &event.paths.get(0);
      if let Some(path) = path {
        if let Err(err) = create(path, client).await {
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
pub(crate) fn spawn(client: &NanocldClient) {
  let client = client.clone();
  rt::Arbiter::new().exec_fn(move || {
    rt::spawn(async move {
      if let Err(err) = watch(&client).await {
        log::warn!("metric::spawn: {err}");
      }
      rt::Arbiter::current().stop();
    });
  });
}
