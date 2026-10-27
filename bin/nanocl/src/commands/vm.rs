#[cfg(not(target_os = "windows"))]
use std::os::fd::AsRawFd;
use std::{
  io::{Read, Write},
  thread,
  time::Duration,
};

use futures::{
  channel::mpsc,
  {SinkExt, StreamExt},
};
use ntex::{rt, time, util::Bytes, ws};
#[cfg(not(target_os = "windows"))]
use termios::{ECHO, ICANON, TCSANOW, Termios, tcsetattr};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericListQueryNsp},
    process::{OutputKind, OutputLog},
    resource_key::ResourceKey,
    system::{EventActorKind, NativeEventAction},
    vm::VmInspect,
    vm_spec::{VmCloudInitUserData, VmSpecPartial, VmSpecUpdate},
  },
};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, GenericHistoryOpts, SpecHistoryRow, VmArg, VmCommand,
    VmCompactRow, VmCreateOpts, VmLifecycleOpts, VmPatchOpts, VmRow, VmRunOpts,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
  GenericCommandStart, GenericCommandStop,
  history::{print_revision_history, validate_options},
};

impl GenericCommand for VmArg {
  fn object_name() -> &'static str {
    "vms"
  }
}

impl GenericCommandLs for VmArg {
  type Item = VmRow;
  type CompactItem = VmCompactRow;
  type Args = VmArg;
  type ApiItem = nanocld_client::stubs::vm::VmSummary;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }

  fn transform_filter(
    _args: &Self::Args,
    filter: &GenericFilter,
    namespace: Option<&str>,
  ) -> impl serde::Serialize {
    GenericListQueryNsp::try_from(filter.clone())
      .unwrap()
      .with_namespace(namespace)
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for VmArg {}

impl GenericCommandStart for VmArg {}

impl GenericCommandStop for VmArg {}

impl GenericCommandInspect for VmArg {
  type ApiItem = VmInspect;
}

async fn wait_vm_state(
  key: &str,
  action: NativeEventAction,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let waiter = utils::process::wait_process_state(
    key,
    EventActorKind::Vm,
    [action].to_vec(),
    client,
  )
  .await?;
  Ok(waiter)
}

fn read_cloud_init_user_data(reader: impl Read) -> IoResult<String> {
  const MAX_BYTES: usize = 64 * 1024;
  let mut data = Vec::new();
  reader
    .take((MAX_BYTES + 1) as u64)
    .read_to_end(&mut data)
    .map_err(|_| {
      IoError::invalid_input("Cloud-init user-data", "Unable to read file")
    })?;
  if data.len() > MAX_BYTES {
    return Err(IoError::invalid_input(
      "Cloud-init user-data",
      "File exceeds the 64 KiB limit",
    ));
  }
  if data.contains(&0) {
    return Err(IoError::invalid_input(
      "Cloud-init user-data",
      "File must not contain NUL bytes",
    ));
  }
  String::from_utf8(data).map_err(|_| {
    IoError::invalid_input("Cloud-init user-data", "File must be UTF-8")
  })
}

fn load_cloud_init_user_data(path: Option<&str>) -> IoResult<Option<String>> {
  path
    .map(|path| {
      let file = std::fs::File::open(path).map_err(|err| {
        err.map_err_context(|| "Unable to open cloud-init user-data file")
      })?;
      read_cloud_init_user_data(file)
    })
    .transpose()
}

/// Function executed when running `nanocl vm create`
/// It will create a new virtual machine but not start it
pub async fn exec_vm_create(
  cli_conf: &CliConfig,
  _args: &VmArg,
  options: &VmCreateOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut options = options.clone();
  let image_full_path = utils::path::resolve_full_path(&options.image)?;
  options.image = image_full_path;
  let mut vm: VmSpecPartial = options.clone().into();
  vm.cloud_init_user_data =
    load_cloud_init_user_data(options.cloud_init_user_data.as_deref())?
      .map(VmCloudInitUserData::Raw);
  let vm = client.create_vm(&vm, options.namespace.as_deref()).await?;
  println!("{}", &vm.spec.vm_key);
  Ok(())
}

/// Function executed when running `nanocl vm run`
/// It will create a new virtual machine, start it.
/// If the `attach` option is set, it will attach to the virtual machine console.
pub async fn exec_vm_run(
  cli_conf: &CliConfig,
  _args: &VmArg,
  options: &VmRunOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut options = options.clone();
  let image_full_path = utils::path::resolve_full_path(&options.image)?;
  options.image = image_full_path;
  let mut vm: VmSpecPartial = options.clone().into();
  vm.cloud_init_user_data =
    load_cloud_init_user_data(options.cloud_init_user_data.as_deref())?
      .map(VmCloudInitUserData::Raw);
  let vm = client.create_vm(&vm, options.namespace.as_deref()).await?;
  let waiter =
    wait_vm_state(&vm.spec.vm_key, NativeEventAction::Start, client).await?;
  client.start_process("vm", &vm.spec.vm_key).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_process_state", &err.to_string())
  })??;
  ntex::time::sleep(Duration::from_secs(10)).await;
  if options.attach {
    #[cfg(not(target_os = "windows"))]
    {
      exec_vm_attach(cli_conf, &vm.spec.vm_key).await?;
    }
    #[cfg(target_os = "windows")]
    {
      println!("Attach is not supported on windows yet");
    }
  }
  Ok(())
}

/// Function executed when running `nanocl vm patch`
/// It will patch a virtual machine with the provided options
pub async fn exec_vm_patch(
  cli_conf: &CliConfig,
  _args: &VmArg,
  options: &VmPatchOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut vm: VmSpecUpdate = options.clone().into();
  vm.cloud_init_user_data = if options.clear_cloud_init_user_data {
    Some(VmCloudInitUserData::Reset)
  } else {
    load_cloud_init_user_data(options.cloud_init_user_data.as_deref())?
      .map(VmCloudInitUserData::Raw)
  };
  let waiter =
    wait_vm_state(&options.key, NativeEventAction::Start, client).await?;
  client.patch_vm(&options.key, &vm).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_process_state", &err.to_string())
  })??;
  Ok(())
}

/// Execute VM history using the shared history selection and display.
async fn exec_vm_history(
  cli_conf: &CliConfig,
  opts: &GenericHistoryOpts,
) -> IoResult<()> {
  validate_options(opts)?;
  let mut histories = cli_conf.client.list_history_vm(&opts.key).await?;
  histories.sort_by_key(|revision| {
    std::cmp::Reverse((revision.created_at, revision.key))
  });
  print_revision_history(
    opts,
    &cli_conf.user_config.display_format,
    histories,
    |revision| revision.key,
    SpecHistoryRow::from,
  )
}

fn vm_process_name(key: &str) -> IoResult<String> {
  let key = key
    .parse::<ResourceKey>()
    .map_err(|err| IoError::invalid_input("VM key", &err.to_string()))?;
  Ok(format!("{key}.v"))
}

async fn exec_vm_lifecycle(
  client: &NanocldClient,
  options: &VmLifecycleOpts,
  restart: bool,
) -> IoResult<()> {
  let request = options.into();
  let request = &request;
  run_vm_lifecycle(&options.keys, |key| async move {
    if restart {
      client.restart_vm(key, request).await.map(|_| ())
    } else {
      client.stop_vm(key, request).await.map(|_| ())
    }
    .map_err(IoError::from)
  })
  .await
}

async fn run_vm_lifecycle<'a, F, Fut>(
  keys: &'a [String],
  mut action: F,
) -> IoResult<()>
where
  F: FnMut(&'a str) -> Fut,
  Fut: std::future::Future<Output = IoResult<()>>,
{
  let mut first_error = None;
  for key in keys {
    match action(key).await {
      Ok(()) => println!("{key}"),
      Err(error) => {
        eprintln!("{key}: {error}");
        first_error.get_or_insert(error);
      }
    }
  }
  first_error.map_or(Ok(()), Err)
}

/// Function executed when running `nanocl vm attach`
/// It will attach to a virtual machine console
#[cfg(not(target_os = "windows"))]
pub async fn exec_vm_attach(cli_conf: &CliConfig, key: &str) -> IoResult<()> {
  let client = &cli_conf.client;
  /// How often heartbeat pings are sent
  const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
  let process_name = vm_process_name(key)?;
  let conn = client.attach_process(&process_name).await?;
  let (mut tx, mut rx) = mpsc::unbounded();
  // start heartbeat task
  let sink = conn.sink();
  rt::spawn(async move {
    loop {
      time::sleep(HEARTBEAT_INTERVAL).await;
      if sink.send(ws::Message::Ping(Bytes::new())).await.is_err() {
        return;
      }
    }
  });
  // // Get the current terminal settings
  let mut termios = Termios::from_fd(std::io::stdin().as_raw_fd())?;
  // Save a copy of the original terminal settings
  let original_termios = termios;
  // Disable canonical mode and echo
  termios.c_lflag &= !(ICANON | ECHO);
  // Redirect the output of the console to the TTY device
  let mut stderr = std::io::stderr();
  let mut stdout = std::io::stdout();
  // let mut tty_writer = std::io::BufWriter::new(tty_file);
  // std::io::copy(&mut stdout, &mut tty_writer)?;
  // Apply the new terminal settings
  tcsetattr(std::io::stdin().as_raw_fd(), TCSANOW, &termios)?;
  // start console read loop
  thread::spawn(move || {
    loop {
      let mut input = [0; 1];
      if std::io::stdin().read(&mut input).is_err() {
        println!("Unable to read stdin");
        return;
      }
      let s = std::str::from_utf8(&input).unwrap();
      // send text to server
      if futures::executor::block_on(tx.send(ws::Message::Text(s.into())))
        .is_err()
      {
        return;
      }
    }
  });
  // read console commands
  let sink = conn.sink();
  rt::spawn(async move {
    while let Some(msg) = rx.next().await {
      if sink.send(msg).await.is_err() {
        return;
      }
    }
  });
  // run ws dispatcher
  let sink = conn.sink();
  let mut rx = conn.seal().receiver();
  while let Some(frame) = rx.next().await {
    match frame {
      Ok(ws::Frame::Binary(text)) => {
        let output =
          serde_json::from_slice::<OutputLog>(&text).map_err(|err| {
            err.map_err_context(|| "Unable to serialize output")
          })?;
        match &output.kind {
          OutputKind::StdOut => {
            stdout.write_all(output.data.as_bytes())?;
            stdout.flush()?;
          }
          OutputKind::StdErr => {
            stderr.write_all(output.data.as_bytes())?;
            stdout.flush()?;
          }
          OutputKind::Console => {
            stdout.write_all(output.data.as_bytes())?;
            stdout.flush()?;
          }
          _ => {}
        }
      }
      Ok(ws::Frame::Ping(msg)) => {
        sink
          .send(ws::Message::Pong(msg))
          .await
          .map_err(std::io::Error::other)?;
      }
      Err(_) => break,
      _ => (),
    }
  }
  // Restore the original terminal settings
  tcsetattr(std::io::stdin().as_raw_fd(), TCSANOW, &original_termios)?;
  Ok(())
}

/// Function executed when running `nanocl vm`
/// It will execute the subcommand passed as argument
pub async fn exec_vm(cli_conf: &CliConfig, args: &VmArg) -> IoResult<()> {
  let client = &cli_conf.client;
  match &args.command {
    VmCommand::Create(options) => exec_vm_create(cli_conf, args, options).await,
    VmCommand::List(opts) => {
      VmArg::exec_ls(client, args, &opts.list, opts.namespace.as_deref()).await
    }
    VmCommand::Remove(opts) => VmArg::exec_rm(&cli_conf.client, opts).await,
    VmCommand::Inspect(opts) => VmArg::exec_inspect(cli_conf, opts).await,
    VmCommand::History(opts) => exec_vm_history(cli_conf, opts).await,
    VmCommand::Start(opts) => VmArg::exec_start(client, opts).await,
    VmCommand::Stop(opts) => exec_vm_lifecycle(client, opts, false).await,
    VmCommand::Restart(opts) => exec_vm_lifecycle(client, opts, true).await,
    VmCommand::Status { key } => {
      let status = client.vm_status(key).await?;
      println!(
        "{}",
        serde_json::to_string_pretty(&status)
          .map_err(|err| err.map_err_context(|| "VM runtime status"))?
      );
      Ok(())
    }
    VmCommand::Resize(options) => {
      let resources = client.resize_vm(&options.key, &options.into()).await?;
      println!(
        "{}",
        serde_json::to_string_pretty(&resources)
          .map_err(|err| err.map_err_context(|| "VM resources"))?
      );
      Ok(())
    }
    VmCommand::Resources { key } => {
      let resources = client.vm_resources(key).await?;
      println!(
        "{}",
        serde_json::to_string_pretty(&resources)
          .map_err(|err| err.map_err_context(|| "VM resources"))?
      );
      Ok(())
    }
    VmCommand::Run(options) => exec_vm_run(cli_conf, args, options).await,
    VmCommand::Patch(options) => exec_vm_patch(cli_conf, args, options).await,
    VmCommand::Attach { key } => {
      #[cfg(not(target_os = "windows"))]
      {
        exec_vm_attach(cli_conf, key).await
      }
      #[cfg(target_os = "windows")]
      {
        println!("Attach is not supported on windows yet");
        Ok(())
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{
    load_cloud_init_user_data, read_cloud_init_user_data, run_vm_lifecycle,
    vm_process_name,
  };
  use nanocl_error::io::IoError;

  #[test]
  fn vm_lifecycle_attempts_all_keys_and_returns_first_failure() {
    let keys =
      ["global.first", "global.second", "global.third"].map(String::from);
    let mut attempted = Vec::new();
    let result = futures::executor::block_on(run_vm_lifecycle(&keys, |key| {
      attempted.push(key.to_owned());
      std::future::ready(if key == "global.first" || key == "global.third" {
        Err(IoError::other("VM lifecycle", key))
      } else {
        Ok(())
      })
    }));
    assert_eq!(attempted, keys);
    assert_eq!(result.unwrap_err().inner.to_string(), "global.first");
    let mut completed = 0;
    futures::executor::block_on(run_vm_lifecycle(&keys, |_| {
      completed += 1;
      std::future::ready(Ok(()))
    }))
    .unwrap();
    assert_eq!(completed, keys.len());
  }

  #[test]
  fn vm_cloud_init_user_data_preserves_raw_utf8_and_empty_input() {
    let data = "#cloud-config\r\nwrite_files:\n- content: café '$HOME'\\n\n";
    assert_eq!(read_cloud_init_user_data(data.as_bytes()).unwrap(), data);
    assert_eq!(read_cloud_init_user_data(&b""[..]).unwrap(), "");
    assert_eq!(load_cloud_init_user_data(None).unwrap(), None);
    let script = "#!/bin/sh\nprintf '%s' custom > /etc/provisioned";
    assert_eq!(
      read_cloud_init_user_data(script.as_bytes()).unwrap(),
      script
    );
  }

  #[test]
  fn vm_cloud_init_user_data_enforces_byte_limit_and_redacts_invalid_data() {
    let data = "é".repeat(32 * 1024);
    assert_eq!(read_cloud_init_user_data(data.as_bytes()).unwrap(), data);
    let too_large = format!("{data}x");
    let error = read_cloud_init_user_data(too_large.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("64 KiB"));
    assert!(!error.to_string().contains(&data));
    let mut oversized = std::io::Cursor::new(vec![b'x'; 128 * 1024]);
    assert!(read_cloud_init_user_data(&mut oversized).is_err());
    assert_eq!(oversized.position(), 64 * 1024 + 1);
    for data in [
      &b"sensitive-user-data\0"[..],
      &b"sensitive-user-data\xff"[..],
    ] {
      let error = read_cloud_init_user_data(data).unwrap_err().to_string();
      assert!(!error.contains("sensitive-user-data"));
    }
  }

  #[test]
  fn vm_attach_maps_canonical_key_to_runtime_process_name() {
    assert_eq!(
      vm_process_name("team.production.database").unwrap(),
      "team.production.database.v"
    );
    assert!(vm_process_name("database").is_err());
  }
}
