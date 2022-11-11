pub mod common;

#[cfg(test)]
pub mod version {
  use super::common;

  #[ntex::test]
  async fn exec_version() -> Result<(), common::TestError> {
    let output = common::spawn_cli(vec!["version"]).await?;
    println!("{:#?}", &output);
    assert!(output.status.success());
    Ok(())
  }
}
