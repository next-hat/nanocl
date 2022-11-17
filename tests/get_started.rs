pub mod common;
use serde_json;
use crate::common::TestResult;

async fn exec_run() -> TestResult<()> {
  let output = common::exec_nanocl(vec![
    "run",
    "--cluster",
    "dev",
    "--network",
    "front",
    "--image",
    "nginx:1.23",
    "my-cargo",
  ])
  .await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_inspect() -> TestResult<()> {
  let output =
    common::exec_nanocl(vec!["cargo", "inspect", "my-cargo"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_list() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cargo", "ls"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_help() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cargo", "help"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cluster_list() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cluster", "ls"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cluster_inspect() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cluster", "inspect", "dev"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cluster_help() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cluster", "help"]).await?;
  assert!(output.status.success());
  Ok(())
}

async fn download_get_started_image() -> TestResult<()> {
  let output = common::exec_nanocl(vec![
    "cargo",
    "image",
    "create",
    "nexthat/nanocl-get-started:latest",
  ])
  .await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_patch_image() -> TestResult<()> {
  // nanocl cargo patch my-cargo set --image get-started:master
  let output = common::exec_nanocl(vec![
    "cargo",
    "patch",
    "my-cargo",
    "set",
    "--image",
    "nexthat/nanocl-get-started:latest",
  ])
  .await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_patch_env_port() -> TestResult<()> {
  let output = common::exec_nanocl(vec![
    "cargo",
    "patch",
    "my-cargo",
    "set",
    "--env",
    "PORT=9001",
  ])
  .await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cluster_variable_create() -> TestResult<()> {
  let output = common::exec_nanocl(vec![
    "cluster", "variable", "dev", "create", "CLUSTER", "DEV",
  ])
  .await?;
  assert!(output.status.success());
  Ok(())
}

async fn exec_cargo_patch_env_cluster() -> TestResult<()> {
  let output = common::exec_nanocl(vec![
    "cargo",
    "patch",
    "my-cargo",
    "set",
    "--env",
    "CLUSTER={{vars.CLUSTER}}",
  ])
  .await?;
  assert!(output.status.success());
  let response = common::curl_cargo_instance("my-cargo", "9001").await?;

  let json_resp = serde_json::from_str::<serde_json::Value>(&response).unwrap();

  let cluster = json_resp
    .get("env")
    .unwrap()
    .get("CLUSTER")
    .unwrap()
    .as_str()
    .unwrap()
    .to_owned();

  assert_eq!(cluster, "DEV");

  Ok(())
}

async fn clean() -> TestResult<()> {
  let output = common::exec_nanocl(vec!["cluster", "rm", "dev"]).await?;
  assert!(output.status.success());
  let output = common::exec_nanocl(vec!["cargo", "rm", "my-cargo"]).await?;
  assert!(output.status.success());
  Ok(())
}

#[ntex::test]
async fn scenario() -> TestResult<()> {
  // Ensure Proxy and Dns controller are installed
  common::exec_nanocl(vec!["controller", "add", "proxy"]).await?;
  common::exec_nanocl(vec!["controller", "add", "dns"]).await?;
  exec_run().await?;
  exec_cargo_inspect().await?;
  common::curl_cargo_instance("my-cargo", "80").await?;
  exec_cargo_list().await?;
  exec_cargo_help().await?;
  exec_cluster_list().await?;
  exec_cluster_inspect().await?;
  exec_cluster_help().await?;
  download_get_started_image().await?;
  exec_cargo_patch_image().await?;
  exec_cargo_inspect().await?;
  common::curl_cargo_instance("my-cargo", "9000").await?;
  exec_cargo_patch_env_port().await?;
  exec_cargo_inspect().await?;
  common::curl_cargo_instance("my-cargo", "9001").await?;
  exec_cluster_variable_create().await?;
  exec_cargo_patch_env_cluster().await?;
  clean().await?;
  Ok(())
}
