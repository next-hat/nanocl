// pub mod common;
// use crate::common::TestResult;

// async fn test_create() -> TestResult<()> {
//   common::exec_nanocl(vec!["cargo", "image", "create", "busybox:1.35.0-musl"])
//     .await?;
//   Ok(())
// }

// async fn test_list() -> TestResult<()> {
//   common::exec_nanocl(vec!["cargo", "image", "ls"]).await?;
//   Ok(())
// }

// async fn test_inspect() -> TestResult<()> {
//   common::exec_nanocl(vec!["cargo", "image", "inspect", "busybox:1.35.0-musl"])
//     .await?;
//   Ok(())
// }

// async fn test_remove() -> TestResult<()> {
//   common::exec_nanocl(vec!["cargo", "image", "rm", "busybox:1.35.0-musl"])
//     .await?;
//   Ok(())
// }

// #[ntex::test]
// async fn test() -> TestResult<()> {
//   test_create().await?;
//   test_list().await?;
//   test_inspect().await?;
//   test_remove().await?;
//   Ok(())
// }
