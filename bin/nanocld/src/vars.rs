/// This arch is the target architecture of the binary.
/// eg:
/// * x86_64
/// * aarch64
pub const ARCH: &str = env!("TARGET_ARCH");
/// This version is the version of the binary.
/// * x.x.x
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// This commit id is the commit id of the binary.
pub const COMMIT_ID: &str = env!("GIT_HASH");
/// This channel is the channel of the binary.
/// eg:
/// * stable
/// * nightly
pub const CHANNEL: &str = env!("CHANNEL");
/// Controller
pub const CONTROLLER_NAME: &str = "nanocl.io/core";
/// Select the VM runtime from the daemon's existing release channel.
pub fn vm_runtime(channel: &str) -> &'static str {
  match channel {
    "nightly" => "ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly",
    _ => "ghcr.io/next-hat/nanocl-qemu:8.0.5.0",
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn vm_runtime_uses_the_daemon_release_channel() {
    assert_eq!(
      super::vm_runtime("nightly"),
      "ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly"
    );
    for channel in ["stable", "custom"] {
      assert_eq!(
        super::vm_runtime(channel),
        "ghcr.io/next-hat/nanocl-qemu:8.0.5.0"
      );
    }
  }
}
