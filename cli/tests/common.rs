use std::fmt::Debug;
use std::{thread, time};
use std::process::{Command, Stdio};

use ntex::rt;

pub struct TestError {
  pub(crate) msg: String,
}

impl Debug for TestError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TestError").field("msg", &self.msg).finish()
  }
}

pub type TestResult<T> = Result<T, TestError>;

pub async fn exec_command(
  cmd: &str,
  args: Vec<String>,
) -> TestResult<std::process::Output> {
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

pub async fn exec_nanocl(args: Vec<&str>) -> TestResult<std::process::Output> {
  let args = args.into_iter().map(|item| item.to_owned()).collect();
  exec_command("./target/debug/nanocl", args).await
}

pub fn sleep_milli(time: u64) {
  thread::sleep(time::Duration::from_millis(time));
}

pub async fn get_cargo_ip_addr(name: &str) -> TestResult<String> {
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
  let ip_addr = String::from_utf8(output.stdout.to_vec())
    .unwrap()
    .trim()
    .to_owned();
  Ok(ip_addr)
}

pub async fn exec_curl(args: Vec<&str>) -> TestResult<String> {
  println!("Executing curl with args : {}", &args.join(" "));
  let output = exec_command(
    "bash",
    vec![String::from("-c"), format!("curl {}", &args.join(" "))],
  )
  .await?;

  println!("{:#?}", &output);

  assert!(output.status.success());

  let output = String::from_utf8(output.stdout.to_vec())
    .unwrap()
    .trim()
    .to_owned();
  println!("[OUTPUT]\n{output}");
  Ok(output)
}

pub async fn curl_cargo_instance(name: &str, port: &str) -> TestResult<String> {
  let ip_addr = get_cargo_ip_addr(name).await?;
  let host = format!("http://{}:{}", &ip_addr, port);
  exec_curl(vec![&host]).await
}
