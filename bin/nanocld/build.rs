use nanocl_utils::build_tools::*;

fn main() -> std::io::Result<()> {
  set_channel()?;
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  std::env::set_var("RUST_BACKTRACE", "FULL");
  Ok(())
}
