use std::fs;
use std::io::{Result, Error, ErrorKind};

use clap::*;

include!("./src/models/mod.rs");

/// Man page name and command to generate
struct ManPage<'a> {
  name: &'a str,
  command: clap::Command,
}

/// Path where to generate the files
const MAN_PATH: &str = "./target/man";

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

/// Function to generate a man page
fn generate_man_page<'a>(name: &'a str, app: &'a clap::Command) -> Result<()> {
  let man = clap_mangen::Man::new(app.to_owned());
  // clap_mangen::multiple
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(out_dir.join(format!("{MAN_PATH}/{name}.1")), man_buffer)?;

  Ok(())
}

/// Generate all man pages
pub fn generate_man_pages() -> Result<()> {
  let man_pages: Vec<ManPage> = vec![
    ManPage {
      name: "nanocl",
      command: Cli::command(),
    },
    ManPage {
      name: "nanocl-namespace",
      command: NamespaceArgs::command(),
    },
    ManPage {
      name: "nanocl-cargo",
      command: CargoArgs::command(),
    },
    ManPage {
      name: "nanocl-cargo-image",
      command: CargoImageOpts::command(),
    },
    ManPage {
      name: "nanocl-cargo-run",
      command: CargoRunOpts::command(),
    },
    ManPage {
      name: "nanocl-vm",
      command: VmArgs::command(),
    },
    ManPage {
      name: "nanocl-vm-run",
      command: VmRunOpts::command(),
    },
    ManPage {
      name: "nanocl-state",
      command: StateArgs::command(),
    },
    ManPage {
      name: "nanocl-state-apply",
      command: StateApplyOpts::command(),
    },
    ManPage {
      name: "nanocl-state-remove",
      command: StateRemoveOpts::command(),
    },
    ManPage {
      name: "nanocl-resource",
      command: ResourceArgs::command(),
    },
    ManPage {
      name: "nanocl-setup",
      command: InstallOpts::command(),
    },
  ];

  fs::create_dir_all(MAN_PATH)?;
  for page in man_pages {
    generate_man_page(page.name, &page.command)?;
  }
  Ok(())
}

fn set_channel() -> Result<()> {
  #[allow(unused)]
  let mut default_channel = "stable";
  #[cfg(any(feature = "dev", feature = "test"))]
  {
    default_channel = "nightly";
  }
  let channel =
    std::env::var("NANOCL_CHANNEL").unwrap_or(default_channel.into());
  println!("cargo:rustc-env=CHANNEL={channel}");
  Ok(())
}

fn main() -> Result<()> {
  set_env_target_arch()?;
  set_channel()?;
  set_env_git_commit_hash()?;
  generate_man_pages()?;
  Ok(())
}
