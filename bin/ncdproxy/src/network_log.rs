/// Watch for change inside the access log directory
/// and print them to the console
/// that way we can see what is happening in real time with docker logs
/// so the daemon will be able to save them to the database
/// and broadcast them in real time
use std::fs::File;
use std::path::Path;
use std::time::Duration;
use std::io::{Read, Seek, BufReader, SeekFrom};

use ntex::rt;
use notify::{Config, Watcher, RecursiveMode, RecommendedWatcher};

pub fn print_last_line(path: &Path) {
  if !path.exists() {
    return;
  }
  let file = File::open(path).unwrap();
  let mut buf_reader = BufReader::new(file);

  let mut pos = buf_reader.seek(SeekFrom::End(-2)).unwrap();
  let mut last_line = String::new();

  while pos > 0 {
    buf_reader.seek(SeekFrom::Start(pos)).unwrap();
    let mut buffer = [0; 1];
    let res = buf_reader.read_exact(&mut buffer);
    if buffer[0] == b'\n' || res.is_err() {
      break;
    }
    last_line.insert(0, buffer[0] as char);
    pos -= 1;
  }

  let file_name = path.file_name().unwrap().to_str().unwrap();

  log::debug!("{}", file_name);
  match file_name {
    "http.log" => {
      println!("#HTTP {last_line}");
    }
    "stream.log" => {
      println!("#STREAM {last_line}");
    }
    _ => {}
  }
}

pub(crate) fn run() {
  rt::Arbiter::new().exec_fn(|| {
    let path = Path::new("/var/log/nginx/access");
    if !path.exists() {
      log::debug!("{} doesn't exists", path.display());
    }
    let (tx, rx) = std::sync::mpsc::channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(
      tx,
      Config::default()
        .with_compare_contents(true)
        .with_poll_interval(Duration::from_secs(2)),
    )
    .unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path, RecursiveMode::Recursive).unwrap();

    log::debug!("watching change of: {}", path.display());
    for res in rx {
      match res {
        Ok(event) => match &event.kind {
          notify::EventKind::Modify(e) => match e {
            notify::event::ModifyKind::Data(_) => {
              log::debug!("modified event: {:?}", event);
              let path = &event.paths.get(0);
              if let Some(path) = path {
                print_last_line(path)
              }
            }
            notify::event::ModifyKind::Other => {}
            _ => {}
          },
          notify::EventKind::Other => {
            log::debug!("other");
          }
          _ => {}
        },
        Err(e) => println!("watch error: {:?}", e),
      }
    }
  });
}
