use std::{
  thread,
  io::{Read, Write},
  os::fd::AsRawFd,
  time::Duration,
};

use ntex::{rt, ws, time, util::Bytes};
use futures::{
  channel::mpsc,
  {SinkExt, StreamExt},
};
use termios::{TCSANOW, tcsetattr, Termios, ICANON, ECHO};

use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::stubs::process::{OutputLog, OutputKind};

use crate::{
  utils,
  config::CliConfig,
  models::{
    GenericDefaultOpts, VmArg, VmCommand, VmCreateOpts, VmInspectOpts,
    VmPatchOpts, VmRow, VmRunOpts,
  },
};

use super::{GenericList, GenericRemove};
use super::vm_image::exec_vm_image;

impl GenericList for VmArg {
  type Item = VmRow;
  type Args = VmArg;
  type ApiItem = nanocld_client::stubs::vm::VmSummary;

  fn object_name() -> &'static str {
    "vms"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericRemove<GenericDefaultOpts, String> for VmArg {
  fn object_name() -> &'static str {
    "vms"
  }
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

/// Function executed when running `nanocl vm inspect`
/// It will inspect a virtual machine
/// and output the result on stdout as yaml, toml or json
/// depending on user configuration
pub async fn exec_vm_inspect(
  cli_conf: &CliConfig,
  args: &VmArg,
  opts: &VmInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let vm = client
    .inspect_vm(&opts.name, args.namespace.as_deref())
    .await?;
  let display = opts
    .display
    .clone()
    .unwrap_or(cli_conf.user_config.display_format.clone());
  utils::print::display_format(&display, vm)?;
  Ok(())
}

/// Function executed when running `nanocl vm start`
/// It will start a virtual machine that was previously created or stopped
pub async fn exec_vm_start(
  cli_conf: &CliConfig,
  args: &VmArg,
  names: &[String],
) -> IoResult<()> {
  let client = &cli_conf.client;
  for name in names {
    if let Err(err) = client
      .start_process("vm", name, args.namespace.as_deref())
      .await
    {
      eprintln!("{name}: {err}");
    }
  }
  Ok(())
}

/// Function executed when running `nanocl vm stop`
/// It will stop a virtual machine that was previously started
pub async fn exec_vm_stop(
  cli_conf: &CliConfig,
  args: &VmArg,
  names: &[String],
) -> IoResult<()> {
  let client = &cli_conf.client;
  for name in names {
    if let Err(err) = client
      .stop_process("vm", name, args.namespace.as_deref())
      .await
    {
      eprintln!("{name}: {err}");
    }
  }
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
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.as_deref()).await?;
  client
    .start_process("vm", &vm.spec.name, args.namespace.as_deref())
    .await?;
  if options.attach {
    exec_vm_attach(cli_conf, args, &options.name).await?;
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
  let vm = options.clone().into();
  client
    .patch_vm(&options.name, &vm, args.namespace.as_deref())
    .await?;
  Ok(())
}

/// Function executed when running `nanocl vm attach`
/// It will attach to a virtual machine console
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
  match &args.command {
    VmCommand::Image(args) => exec_vm_image(client, args).await,
    VmCommand::Create(options) => exec_vm_create(cli_conf, args, options).await,
    VmCommand::List(opts) => VmArg::exec_ls(client, args, opts).await,
    VmCommand::Remove(opts) => {
      VmArg::exec_rm(
        &cli_conf.client,
        opts,
        Some(args.namespace.clone().unwrap_or("global".to_owned())),
      )
      .await
    }
    VmCommand::Inspect(opts) => exec_vm_inspect(cli_conf, args, opts).await,
    VmCommand::Start(opts) => exec_vm_start(cli_conf, args, &opts.names).await,
    VmCommand::Stop(opts) => exec_vm_stop(cli_conf, args, &opts.names).await,
    VmCommand::Run(options) => exec_vm_run(cli_conf, args, options).await,
    VmCommand::Patch(options) => exec_vm_patch(cli_conf, args, options).await,
    VmCommand::Attach { name } => exec_vm_attach(cli_conf, args, name).await,
  }
}
