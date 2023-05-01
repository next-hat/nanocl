pub const ARCH: &str = env!("TARGET_ARCH");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT_ID: &str = env!("GIT_HASH");
pub const CHANNEL: &str = env!("CHANNEL");

pub fn print_version() {
  println!("Arch: {ARCH}");
  println!("Channel: {CHANNEL}");
  println!("Version: {VERSION}");
  println!("Commit ID: {COMMIT_ID}");
}
