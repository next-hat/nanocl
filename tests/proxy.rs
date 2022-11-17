pub mod common;
use common::TestResult;

/// Test to run a cargo using get started image with a proxy config
#[ntex::test]
async fn with_get_started() -> TestResult<()> {
  // Ensure the get started image is installed
  let output = common::exec_nanocl(vec![
    "cargo",
    "image",
    "create",
    "nexthat/nanocl-get-started:latest",
  ])
  .await?;
  assert!(output.status.success());

  // List proxy template
  let output = common::exec_nanocl(vec!["proxy", "template", "ls"]).await?;
  assert!(output.status.success());

  // Create a new proxy template test-get-started
  let output = common::exec_nanocl(vec![
    "proxy",
    "template",
    "create",
    "--mode",
    "http",
    "-f",
    "./examples/test_proxy.nginx",
    "test-get-started",
  ])
  .await?;
  assert!(output.status.success());

  // Create a cluster testgst for test get started
  let output =
    common::exec_nanocl(vec!["cluster", "create", "testgst"]).await?;
  assert!(output.status.success());

  // Create a network test inside testgst cluster
  let output = common::exec_nanocl(vec![
    "cluster", "network", "testgst", "create", "test",
  ])
  .await?;
  assert!(output.status.success());

  // Link proxy to cluster testgst
  let output =
    common::exec_nanocl(vec!["proxy", "link", "testgst", "test-get-started"])
      .await?;
  assert!(output.status.success());

  // Create a cargo to use the proxy template
  let output = common::exec_nanocl(vec![
    "cargo",
    "create",
    "--image",
    "nexthat/nanocl-get-started:latest",
    "test-get-started",
  ])
  .await?;
  assert!(output.status.success());

  // Join Cargo inside cluster to create his instances
  let output = common::exec_nanocl(vec![
    "cluster",
    "join",
    "testgst",
    "test",
    "test-get-started",
  ])
  .await?;
  assert!(output.status.success());

  // Start cargo instance of the cluster
  let output = common::exec_nanocl(vec!["cluster", "start", "testgst"]).await?;
  assert!(output.status.success());

  // Test if the proxy config work by pinging a custom domain name
  let output = common::exec_curl(vec![
    "-sv",
    "--resolve",
    &format!("test.get-started.internal:80:127.0.0.1"),
    "http://test.get-started.internal",
  ])
  .await?;
  println!("curl output : {}", &output);

  // Remove the proxy template test-get-started
  let output =
    common::exec_nanocl(vec!["proxy", "template", "rm", "test-get-started"])
      .await?;
  assert!(output.status.success());

  // Remove cluster testgst
  let output = common::exec_nanocl(vec!["cluster", "rm", "testgst"]).await?;
  assert!(output.status.success());

  // Remove cargo test-get-started
  let output =
    common::exec_nanocl(vec!["cargo", "rm", "test-get-started"]).await?;

  assert!(output.status.success());

  Ok(())
}
