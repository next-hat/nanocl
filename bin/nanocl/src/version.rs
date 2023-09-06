/// ## TARGET ARCH
///
/// Target achitecture of the build.
///
pub const ARCH: &str = env!("TARGET_ARCH");
/// ## VERSION
///
/// Version of the build.
///
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// ## COMMIT ID
///
/// Commit ID of the build.
///
pub const COMMIT_ID: &str = env!("GIT_HASH");
/// ## CHANNEL
///
/// Channel of the build.
///
pub const CHANNEL: &str = env!("CHANNEL");

/// ## Print Version
///
/// Prints the version information to the console.
///
pub fn print_version() {
  println!("Arch: {ARCH}");
  println!("Channel: {CHANNEL}");
  println!("Version: {VERSION}");
  println!("Commit ID: {COMMIT_ID}");
}
