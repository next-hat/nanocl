use std::process::{Stdio, Command};

use crate::models::DockerOptions;

use super::errors::CliError;

pub async fn exec_docker(options: &DockerOptions) -> Result<(), CliError> {
  let mut opts = vec![
    String::from("-H"),
    String::from("unix:///run/nanocl/docker.sock"),
  ];
  let mut more_options = options.args.clone();
  opts.append(&mut more_options);

  let mut cmd = Command::new("docker")
    .args(&opts)
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .spawn()
    .unwrap();

  let _status = cmd.wait();
  Ok(())
}
