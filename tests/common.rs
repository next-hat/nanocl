use ntex::rt;
use std::fmt::Debug;
use std::process::{Command, Stdio};

pub struct TestError {
  pub(crate) msg: String,
}

impl Debug for TestError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TestError").field("msg", &self.msg).finish()
  }
}

async fn exec_command(
  cmd: &str,
  args: Vec<String>,
) -> Result<std::process::Output, TestError> {
  let cmd = cmd.to_owned();
  let output = rt::spawn(async move {
    let output = Command::new(&cmd)
      .args(&args)
      .stderr(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()
      .map_err(|err| TestError {
        msg: format!(
          "Unable to spawn command `{} {}`\n\
        got error : {}",
          &cmd,
          args.join(" ").to_string(),
          err,
        ),
      })?
      .wait_with_output()
      .map_err(|err| TestError {
        msg: format!(
          "Unable to wait with output command `{} {}`\n\
        got error : {}",
          &cmd,
          args.join(" ").to_string(),
          err,
        ),
      })?;
    Ok::<std::process::Output, TestError>(output)
  })
  .await
  .map_err(|err| TestError {
    msg: format!("Spawn error {}", &err),
  })??;
  println!("{:#?}", &output);
  Ok(output)
}

pub async fn spawn_cli(
  args: Vec<&str>,
) -> Result<std::process::Output, TestError> {
  let args = args.into_iter().map(|item| item.to_owned()).collect();
  exec_command("./target/debug/nanocl", args).await
}

pub async fn get_cargo_ip_addr(name: &str) -> Result<String, TestError> {
  let path = std::env::current_dir().unwrap();
  let binary_path = format!("{}/target/debug/nanocl", &path.display());
  // cargo run cargo inspect my-cargo | grep -E -o "([0-9]{1,3}[\.]){3}[0-9]{1,3}"
  let bash_expr = binary_path.to_owned()
    + " cargo inspect "
    + name
    + " | grep -E -o '([0-9]{1,3}[\\.]){3}[0-9]{1,3}'";
  let output =
    exec_command("bash", vec![String::from("-c"), bash_expr]).await?;

  assert!(output.status.success());
  let ip_addr = String::from_utf8(output.stdout.to_vec()).unwrap();
  println!("Ip address !! {:?}", &output);
  Ok(ip_addr)
}

pub async fn exec_curl(host: &str) -> Result<(), TestError> {
  println!("curl on host : {}", host);
  let output =
    exec_command("bash", vec![String::from("-c"), format!("curl {}", &host)])
      .await?;
  println!("{:#?}", &output);
  assert!(output.status.success());
  Ok(())
}
