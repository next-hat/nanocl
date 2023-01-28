pub fn print_version() {
  const ARCH: &str = env!("TARGET_ARCH");
  const VERSION: &str = env!("CARGO_PKG_VERSION");
  const COMMIT_ID: &str = env!("GIT_HASH");

  println!("Arch: {ARCH}");
  println!("Version: {VERSION}");
  println!("Commit Id: {COMMIT_ID}");
}
