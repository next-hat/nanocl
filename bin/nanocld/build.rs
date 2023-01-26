use std::fs;
use std::io::{Result, Error, ErrorKind};

use clap::*;

include!("./src/cli.rs");

const MAN_PATH: &str = "../../target/man";

/// Set the git commit hash as an environment variable
fn set_env_git_commit_hash() -> Result<()> {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()?;

  let git_hash = String::from_utf8(output.stdout).unwrap();

  println!("cargo:rustc-env=GIT_HASH={}", git_hash);

  Ok(())
}

/// Set the target architecture as an environment variable
fn set_env_target_arch() -> Result<()> {
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH")
    .map_err(|e| Error::new(ErrorKind::Other, e))?;

  println!("cargo:rustc-env=TARGET_ARCH={}", arch);

  Ok(())
}

/// Generate nanocld man page
pub fn generate_man_command(
  name: &str,
  app: clap::Command,
) -> std::io::Result<()> {
  let man = clap_mangen::Man::new(app);
  // clap_mangen::multiple
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(
    out_dir.join(format!("{MAN_PATH}/{name}.1", name = name)),
    man_buffer,
  )?;

  Ok(())
}

fn main() -> std::io::Result<()> {
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  fs::create_dir_all(MAN_PATH)?;
  generate_man_command("nanocld", Cli::command())?;
  Ok(())
}
