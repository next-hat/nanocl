use nanocl_utils::build_tools::*;

fn main() -> std::io::Result<()> {
  println!("cargo:rerun-if-env-changed=NANOCL_CHANNEL");
  println!("cargo:rerun-if-changed=migrations");
  set_channel()?;
  set_env_target_arch()?;
  set_env_git_commit_hash()?;
  Ok(())
}
