use std::fs;
use std::io::Result;

use clap::*;

use nanocl_utils::build_tools::*;

include!("./src/models/mod.rs");

/// ## MAN PATH
///
/// Path where to render the man pages
///
const MAN_PATH: &str = "./target/man";

fn generate_man_recurr(base_name: &str, app: &clap::Command) -> Result<()> {
  generate_man_page(base_name, app, MAN_PATH)?;
  for subcommand in app.get_subcommands() {
    let name = subcommand.get_name();
    generate_man_page(&format!("{base_name}-{name}"), subcommand, MAN_PATH)?;
    if subcommand.has_subcommands() {
      generate_man_recurr(&format!("{base_name}-{name}"), subcommand)?;
    }
  }
  Ok(())
}

/// ## Generate man pages
///
/// Generate manpage for nanocl and all subcommands
/// and write them inside [MAN_PATH](MAN_PATH)
///
pub fn generate_man_pages() -> Result<()> {
  // ensure the man page directory exists
  fs::create_dir_all(MAN_PATH)?;
  let app = Cli::command();
  generate_man_recurr("nanocl", &app)?;
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
