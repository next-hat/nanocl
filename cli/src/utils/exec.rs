use ntex::rt;
use std::process::{Command, Output};
use std::io::{Error as IoError, ErrorKind};

pub async fn _exec(
  cmd: &str,
  args: &Vec<&str>,
  current_dir: Option<&str>,
) -> Result<Output, IoError> {
  let cmd = cmd.to_owned();
  let args = args.iter().map(|e| e.to_string()).collect::<Vec<String>>();
  let current_dir = match current_dir {
    None => std::env::current_dir()?.display().to_string(),
    Some(current) => current.to_owned(),
  };

  rt::spawn(async {
    let output = Command::new(cmd)
      .args(args)
      .current_dir(current_dir)
      .spawn()?
      .wait_with_output()?;
    Ok::<_, std::io::Error>(output)
  })
  .await
  .map_err(|err| IoError::new(ErrorKind::Other, format!("{err}")))?
}
