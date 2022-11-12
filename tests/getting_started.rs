pub mod common;

#[cfg(test)]
pub mod getting_started {
  use serde_json;
  use crate::common::TestResult;

  use super::common;

  async fn exec_run() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec![
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

  async fn exec_cargo_inspect() -> Result<(), common::TestError> {
    let output =
      common::spawn_cli(vec!["cargo", "inspect", "my-cargo"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cargo_list() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cargo", "ls"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cargo_help() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cargo", "help"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cluster_list() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cluster", "ls"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cluster_inspect() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cluster", "inspect", "dev"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cluster_help() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cluster", "help"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_git_repository_create() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec![
      "git-repository",
      "create",
      "--url",
      "https://github.com/nxthat/nanocl-getting-started",
      "get-started",
    ])
    .await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_git_repository_build() -> Result<(), common::TestError> {
    let output =
      common::spawn_cli(vec!["git-repository", "build", "get-started"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_git_repository_help() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["git-repository", "help"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cargo_patch_image() -> Result<(), common::TestError> {
    // nanocl cargo patch my-cargo set --image get-started:master
    let output = common::spawn_cli(vec![
      "cargo",
      "patch",
      "my-cargo",
      "set",
      "--image",
      "get-started:master",
    ])
    .await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cargo_patch_env_port() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec![
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

  async fn exec_cluster_variable_create() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec![
      "cluster", "variable", "dev", "create", "CLUSTER", "DEV",
    ])
    .await?;
    assert!(output.status.success());
    Ok(())
  }

  async fn exec_cargo_patch_env_cluster() -> TestResult<()> {
    let output = common::spawn_cli(vec![
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

    let json_resp =
      serde_json::from_str::<serde_json::Value>(&response).unwrap();

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

  async fn clean() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cluster", "rm", "dev"]).await?;
    assert!(output.status.success());
    let output =
      common::spawn_cli(vec!["git-repository", "rm", "get-started"]).await?;
    assert!(output.status.success());
    let output = common::spawn_cli(vec!["cargo", "rm", "my-cargo"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  #[ntex::test]
  async fn test_getting_started() -> Result<(), common::TestError> {
    exec_run().await?;
    exec_cargo_inspect().await?;
    common::curl_cargo_instance("my-cargo", "80").await?;
    exec_cargo_list().await?;
    exec_cargo_help().await?;
    exec_cluster_list().await?;
    exec_cluster_inspect().await?;
    exec_cluster_help().await?;
    exec_git_repository_create().await?;
    exec_git_repository_build().await?;
    exec_git_repository_help().await?;
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
}
