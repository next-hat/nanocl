use std::thread;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::time::Duration;

use ntex::{ws, rt, time};
use ntex::util::Bytes;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use termios::{TCSANOW, tcsetattr, Termios, ICANON, ECHO};

use nanocl_utils::io_error::{IoResult, FromIo};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::{OutputLog, OutputKind};

use crate::models::{
  VmArgs, VmCommands, VmCreateOpts, VmRow, VmRunOpts, VmPatchOpts,
};
use crate::utils::print::{print_table, print_yml};

use super::vm_image::exec_vm_image;

pub async fn exec_vm_create(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmCreateOpts,
) -> IoResult<()> {
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.clone()).await?;

  println!("{}", &vm.key);

  Ok(())
}

pub async fn exec_vm_ls(client: &NanocldClient, args: &VmArgs) -> IoResult<()> {
  let items = client.list_vm(args.namespace.clone()).await?;

  let rows = items.into_iter().map(VmRow::from).collect::<Vec<VmRow>>();

  print_table(rows);

  Ok(())
}

pub async fn exec_vm_rm(
  client: &NanocldClient,
  args: &VmArgs,
  names: &[String],
) -> IoResult<()> {
  for name in names {
    client.delete_vm(name, args.namespace.clone()).await?;
  }

  Ok(())
}

pub async fn exec_vm_inspect(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> IoResult<()> {
  let vm = client.inspect_vm(name, args.namespace.clone()).await?;

  print_yml(vm)?;

  Ok(())
}

pub async fn exec_vm_start(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> IoResult<()> {
  client.start_vm(name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_stop(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> IoResult<()> {
  client.stop_vm(name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_run(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmRunOpts,
) -> IoResult<()> {
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.clone()).await?;
  client.start_vm(&vm.name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_patch(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmPatchOpts,
) -> IoResult<()> {
  let vm = options.clone().into();
  client
    .patch_vm(&options.name, &vm, args.namespace.clone())
    .await?;

  Ok(())
}

pub async fn exec_vm_attach(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> IoResult<()> {
  /// How often heartbeat pings are sent
  const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
  let conn = client.attach_vm(name, args.namespace.clone()).await?;
  let (mut tx, mut rx) = mpsc::unbounded();

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

pub async fn exec_vm(client: &NanocldClient, args: &VmArgs) -> IoResult<()> {
  match &args.commands {
    VmCommands::Image(args) => exec_vm_image(client, args).await,
    VmCommands::Create(options) => exec_vm_create(client, args, options).await,
    VmCommands::List => exec_vm_ls(client, args).await,
    VmCommands::Remove { names } => exec_vm_rm(client, args, names).await,
    VmCommands::Inspect { name } => exec_vm_inspect(client, args, name).await,
    VmCommands::Start { name } => exec_vm_start(client, args, name).await,
    VmCommands::Stop { name } => exec_vm_stop(client, args, name).await,
    VmCommands::Run(options) => exec_vm_run(client, args, options).await,
    VmCommands::Patch(options) => exec_vm_patch(client, args, options).await,
    VmCommands::Attach { name } => exec_vm_attach(client, args, name).await,
  }
}
