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
    system::{EventActorKind, NativeEventAction},
    vm::VmInspect,
    vm_spec::VmSpecPartial,
  },
};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, VmArg, VmCommand, VmCreateOpts, VmPatchOpts, VmRow,
    VmRunOpts,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
  GenericCommandStart, GenericCommandStop,
};

impl GenericCommand for VmArg {
  fn object_name() -> &'static str {
    "vms"
  }
}

impl GenericCommandLs for VmArg {
  type Item = VmRow;
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
  let vm = options.clone().into();
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
  let vm: VmSpecPartial = options.clone().into();
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
  let waiter =
    wait_vm_state(&options.key, NativeEventAction::Start, client).await?;
  let vm = options.clone().into();
  client.patch_vm(&options.key, &vm).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_process_state", &err.to_string())
  })??;
  Ok(())
}

/// Function executed when running `nanocl vm attach`
/// It will attach to a virtual machine console
#[cfg(not(target_os = "windows"))]
pub async fn exec_vm_attach(cli_conf: &CliConfig, key: &str) -> IoResult<()> {
  let client = &cli_conf.client;
  /// How often heartbeat pings are sent
  const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
  let conn = client.attach_vm(key).await?;
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
    VmCommand::Start(opts) => VmArg::exec_start(client, opts).await,
    VmCommand::Stop(opts) => VmArg::exec_stop(client, opts).await,
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
