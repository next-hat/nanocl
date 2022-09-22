use std::fs;
use clap::IntoApp;

include!("./src/models/mod.rs");

const MAN_PATH: &str = "./target/man";

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
    out_dir.join(&format!("{MAN_PATH}/{name}.1", name = name)),
    man_buffer,
  )?;

  Ok(())
}

pub fn generate_man() -> std::io::Result<()> {
  generate_man_command("nanocl", Cli::into_app())?;
  generate_man_command("nanocl-namespace", NamespaceArgs::into_app())?;
  generate_man_command(
    "nanocl-namespace-create",
    NamespacePartial::into_app(),
  )?;
  generate_man_command("nanocl-apply", ApplyArgs::into_app())?;
  generate_man_command("nanocl-revert", RevertArgs::into_app())?;
  generate_man_command("nanocl-cluster", ClusterArgs::into_app())?;
  generate_man_command(
    "nanocl-cluster-delete",
    ClusterDeleteOptions::into_app(),
  )?;
  generate_man_command("nanocl-cluster-create", ClusterPartial::into_app())?;
  generate_man_command(
    "nanocl-cluster-start",
    ClusterStartOptions::into_app(),
  )?;
  generate_man_command(
    "nanocl-cluster-inspect",
    ClusterInspectOptions::into_app(),
  )?;
  // NamespaceCommands::into_app()
  // generate_man_command("nanocl-namespace-list")

  Ok(())
}

fn main() -> std::io::Result<()> {
  fs::create_dir_all(MAN_PATH)?;
  generate_man()?;
  Ok(())
}
