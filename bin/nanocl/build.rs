use std::fs;
use std::io::{Result, Error, ErrorKind};

use clap::*;

include!("./src/models/mod.rs");

/// ## ManPage
///
/// Definition of a man page to generate for given command
///
struct ManPage<'a> {
  name: &'a str,
  command: clap::Command,
}

/// ## MAN PATH
///
/// Path where to render the man pages
///
const MAN_PATH: &str = "./target/man";

/// ## Set env git commit hash
///
/// Execute the git command to extract the hash of the current commit
/// and set it as an environment variable for the produced binary
///
fn set_env_git_commit_hash() -> Result<()> {
  let output = std::process::Command::new("git")
    .args(["rev-parse", "HEAD"])
    .output()?;

  let git_hash = String::from_utf8(output.stdout).unwrap();

  println!("cargo:rustc-env=GIT_HASH={git_hash}");

  Ok(())
}

/// ## Set env target arch
///
/// Set the target arch as an environment variable for the produced binary
///
fn set_env_target_arch() -> Result<()> {
  let arch = std::env::var("CARGO_CFG_TARGET_ARCH")
    .map_err(|e| Error::new(ErrorKind::Other, e))?;

  println!("cargo:rustc-env=TARGET_ARCH={arch}");

  Ok(())
}

/// ## Generate man page
///
/// Function to generate a man page
///
/// ## Arguments
///
/// * [name](str) Name of the man page
/// * [app](clap::Command) Command to generate
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](Ok) Operation was successful
///   * [Err](Err) Operation failed
///
fn generate_man_page<'a>(name: &'a str, app: &'a clap::Command) -> Result<()> {
  let man = clap_mangen::Man::new(app.to_owned());
  // clap_mangen::multiple
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(out_dir.join(format!("{MAN_PATH}/{name}.1")), man_buffer)?;
  Ok(())
}

/// ## Generate man pages
///
/// Generate manpage for nanocl and all subcommands
/// and write them inside [MAN_PATH](MAN_PATH)
///
pub fn generate_man_pages() -> Result<()> {
  let man_pages = [
    ManPage {
      name: "nanocl",
      command: Cli::command(),
    },
    ManPage {
      name: "nanocl-namespace",
      command: NamespaceArg::command(),
    },
    ManPage {
      name: "nanocl-cargo",
      command: CargoArg::command(),
    },
    ManPage {
      name: "nanocl-cargo-image",
      command: CargoImageArg::command(),
    },
    ManPage {
      name: "nanocl-cargo-run",
      command: CargoRunOpts::command(),
    },
    ManPage {
      name: "nanocl-vm",
      command: VmArg::command(),
    },
    ManPage {
      name: "nanocl-vm-run",
      command: VmRunOpts::command(),
    },
    ManPage {
      name: "nanocl-state",
      command: StateArg::command(),
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
      command: ResourceArg::command(),
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

/// ## Set channel
///
/// Set the release channel as an environment variable for the produced binary
///
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

/// ## Main
///
/// Main entrypoint of the build script
/// The build script will add some environment variables for the production build.
/// In order to track bug in a better way, the channel,
/// the git commit hash and the target arch will be statically linked into the binary.
///
fn main() -> Result<()> {
  set_env_target_arch()?;
  set_channel()?;
  set_env_git_commit_hash()?;
  generate_man_pages()?;
  Ok(())
}
