use clap::*;

use nanocl_utils::build_tools::*;

include!("./src/cli.rs");

const MAN_PATH: &str = "./target/man";

fn main() -> std::io::Result<()> {
  set_channel()?;
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  std::fs::create_dir_all(MAN_PATH)?;
  generate_man_page("nanocld", &Cli::command(), MAN_PATH)?;
  Ok(())
}
