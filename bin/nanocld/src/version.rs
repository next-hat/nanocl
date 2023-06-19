/// ## Architecture of the processor
/// * x86_64
/// * aarch64
pub const ARCH: &str = env!("TARGET_ARCH");
/// ## The version of Cargo.toml
/// This version is the version of the binary.
/// * x.x.x
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// ## The commit id of the binary
/// This commit id is the commit id of the binary.
pub const COMMIT_ID: &str = env!("GIT_HASH");
/// ## The channel of the binary
/// This channel is the channel of the binary.
/// * stable
/// * nightly
pub const CHANNEL: &str = env!("CHANNEL");
