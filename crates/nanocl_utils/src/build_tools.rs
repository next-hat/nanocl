use std::io::{Result, Error, ErrorKind};

/// Execute the git command to extract the hash of the current commit
/// and set it as an environment variable for the produced binary
pub fn set_env_git_commit_hash() -> Result<()> {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()?;
  let mut git_hash = String::from_utf8(output.stdout).unwrap();
  if git_hash.is_empty() {
    git_hash = "<unknow>".to_owned();
  }
  println!("cargo:rustc-env=GIT_HASH={git_hash}");
  Ok(())
}

/// Set the target arch as an environment variable for the produced binary
pub fn set_env_target_arch() -> Result<()> {
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH")
    .map_err(|e| Error::new(ErrorKind::Other, e))?;
  println!("cargo:rustc-env=TARGET_ARCH={arch}");
  Ok(())
}

/// Set the release channel as an environment variable for the produced binary
pub fn set_channel() -> Result<()> {
  #[allow(unused)]
  let mut default_channel = "stable";
  #[cfg(feature = "dev")]
  {
    default_channel = "nightly";
  }
  let channel =
    std::env::var("NANOCL_CHANNEL").unwrap_or(default_channel.into());
  println!("cargo:rustc-env=CHANNEL={channel}");
  Ok(())
}

/// Function to generate a man page
pub fn generate_man_page<'a>(
  name: &'a str,
  app: &'a clap::Command,
  dir: &str,
) -> Result<()> {
  let man = clap_mangen::Man::new(app.to_owned());
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(out_dir.join(format!("{dir}/{name}.1")), man_buffer)?;
  Ok(())
}
