use std::fs;
use std::io::Result;

use clap::*;

use nanocl_utils::build_tools::*;

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
    generate_man_page(page.name, &page.command, MAN_PATH)?;
  }
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
