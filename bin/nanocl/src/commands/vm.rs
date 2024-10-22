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
use termios::{tcsetattr, Termios, ECHO, ICANON, TCSANOW};

use nanocl_error::io::{FromIo, IoResult};
use nanocld_client::{
  stubs::{
    process::{OutputKind, OutputLog},
    system::{EventActorKind, NativeEventAction},
    vm::VmInspect,
    vm_spec::VmSpecPartial,
  },
  NanocldClient,
};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, VmArg, VmCommand, VmCreateOpts, VmPatchOpts, VmRow,
    VmRunOpts,
  },
  utils,
};

use super::vm_image::exec_vm_image;
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
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for VmArg {}

impl GenericCommandStart for VmArg {}

impl GenericCommandStop for VmArg {}

impl GenericCommandInspect for VmArg {
  type ApiItem = VmInspect;
}

async fn wait_vm_state(
  name: &str,
  args: &VmArg,
  action: NativeEventAction,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let waiter = utils::process::wait_process_state(
    &format!("{}.{}", name, args.namespace.as_deref().unwrap_or("global")),
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
  args: &VmArg,
  options: &VmCreateOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.as_deref()).await?;
  println!("{}", &vm.spec.vm_key);
  Ok(())
}

/// Function executed when running `nanocl vm run`
/// It will create a new virtual machine, start it.
/// If the `attach` option is set, it will attach to the virtual machine console.
pub async fn exec_vm_run(
  cli_conf: &CliConfig,
  args: &VmArg,
  options: &VmRunOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let vm: VmSpecPartial = options.clone().into();
  let waiter =
    wait_vm_state(&vm.name, args, NativeEventAction::Start, client).await?;
  let vm = client.create_vm(&vm, args.namespace.as_deref()).await?;
  client
    .start_process("vm", &vm.spec.name, args.namespace.as_deref())
    .await?;
  waiter.await??;
  if options.attach {
    #[cfg(not(target_os = "windows"))]
    {
      exec_vm_attach(cli_conf, args, &options.name).await?;
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
  args: &VmArg,
  options: &VmPatchOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let waiter =
    wait_vm_state(&options.name, args, NativeEventAction::Start, client)
      .await?;
  let vm = options.clone().into();
  client
    .patch_vm(&options.name, &vm, args.namespace.as_deref())
    .await?;
  waiter.await??;
  Ok(())
}

/// Function executed when running `nanocl vm attach`
/// It will attach to a virtual machine console
#[cfg(not(target_os = "windows"))]
pub async fn exec_vm_attach(
  cli_conf: &CliConfig,
  args: &VmArg,
  name: &str,
) -> IoResult<()> {
  let client = &cli_conf.client;
  /// How often heartbeat pings are sent
  const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
  let conn = client.attach_vm(name, args.namespace.as_deref()).await?;
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
  thread::spawn(move || loop {
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
          .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
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
  let namespace = args.namespace.clone().unwrap_or("global".to_owned());
  match &args.command {
    VmCommand::Image(args) => exec_vm_image(client, args).await,
    VmCommand::Create(options) => exec_vm_create(cli_conf, args, options).await,
    VmCommand::List(opts) => VmArg::exec_ls(client, args, opts).await,
    VmCommand::Remove(opts) => {
      VmArg::exec_rm(&cli_conf.client, opts, Some(namespace.clone())).await
    }
    VmCommand::Inspect(opts) => {
      VmArg::exec_inspect(cli_conf, opts, Some(namespace.clone())).await
    }
    VmCommand::Start(opts) => {
      VmArg::exec_start(client, opts, Some(namespace.clone())).await
    }
    VmCommand::Stop(opts) => {
      VmArg::exec_stop(client, opts, Some(namespace.clone())).await
    }
    VmCommand::Run(options) => exec_vm_run(cli_conf, args, options).await,
    VmCommand::Patch(options) => exec_vm_patch(cli_conf, args, options).await,
    VmCommand::Attach { name } => {
      #[cfg(not(target_os = "windows"))]
      {
        exec_vm_attach(cli_conf, args, name).await
      }
      #[cfg(target_os = "windows")]
      {
        println!("Attach is not supported on windows yet");
        Ok(())
      }
    }
  }
}
