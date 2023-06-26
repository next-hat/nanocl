use std::io::{Result, Error, ErrorKind};

/// Set the git commit hash as an environment variable
fn set_env_git_commit_hash() -> Result<()> {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()?;

  let git_hash = String::from_utf8(output.stdout).unwrap();

  println!("cargo:rustc-env=GIT_HASH={git_hash}");

  Ok(())
}

/// Set the target architecture as an environment variable
fn set_env_target_arch() -> Result<()> {
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH")
    .map_err(|e| Error::new(ErrorKind::Other, e))?;

  println!("cargo:rustc-env=TARGET_ARCH={arch}");

  Ok(())
}

fn set_channel() -> Result<()> {
  let channel = std::env::var("NANOCL_CHANNEL").unwrap_or("stable".into());
  println!("cargo:rustc-env=CHANNEL={channel}");
  Ok(())
}

fn main() -> std::io::Result<()> {
  set_channel()?;
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  Ok(())
}
