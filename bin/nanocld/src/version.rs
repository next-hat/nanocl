/// ## ARCH
///
/// This arch is the target architecture of the binary.
/// eg:
/// * x86_64
/// * aarch64
///
pub const ARCH: &str = env!("TARGET_ARCH");
/// ## VERSION
///
/// This version is the version of the binary.
/// * x.x.x
///
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// ## COMMIT ID
///
/// This commit id is the commit id of the binary.
///
pub const COMMIT_ID: &str = env!("GIT_HASH");
/// ## The channel of the binary
///
/// This channel is the channel of the binary.
/// eg:
/// * stable
/// * nightly
///
pub const CHANNEL: &str = env!("CHANNEL");
