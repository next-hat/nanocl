/// Target achitecture of the build.
pub const ARCH: &str = env!("TARGET_ARCH");

/// Version of the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Commit ID of the build.
pub const COMMIT_ID: &str = env!("GIT_HASH");

/// Channel of the build.
pub const CHANNEL: &str = env!("CHANNEL");

/// Prints the version information to the console.
pub fn print_version() {
  println!("Arch: {ARCH}");
  println!("Channel: {CHANNEL}");
  println!("Version: {VERSION}");
  println!("Commit ID: {COMMIT_ID}");
}
