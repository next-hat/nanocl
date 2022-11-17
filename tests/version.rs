pub mod common;

#[ntex::test]
async fn exec() -> Result<(), common::TestError> {
  let output = common::spawn_cli(vec!["version"]).await?;
  assert!(output.status.success());
  Ok(())
}
