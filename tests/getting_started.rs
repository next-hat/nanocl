pub mod common;

#[cfg(test)]
pub mod getting_started {
  use super::common;

  async fn exec_setup() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["setup"]).await?;
    assert!(output.status.success());
    Ok(())
  }

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

  async fn exec_inspect_cargo() -> Result<(), common::TestError> {
    let output =
      common::spawn_cli(vec!["cargo", "inspect", "my-cargo"]).await?;
    assert!(output.status.success());
    let ip_addr = common::get_cargo_ip_addr("my-cargo").await?;
    println!("ip addr : {}", &ip_addr);
    let host = format!("http://{}", &ip_addr);
    common::exec_curl(&host).await?;
    Ok(())
  }

  async fn exec_clean() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["cluster", "rm", "dev"]).await?;
    assert!(output.status.success());
    Ok(())
  }

  #[ntex::test]
  async fn exec_getting_started() -> Result<(), common::TestError> {
    exec_setup().await?;
    exec_run().await?;
    exec_inspect_cargo().await?;
    exec_clean().await?;
    Ok(())
  }
}
