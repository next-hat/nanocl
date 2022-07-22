use clap::IntoApp;

use crate::cli;
use crate::nanocld::namespace::NamespacePartial;

pub fn generate_man_command(
  name: &str,
  app: clap::Command,
) -> std::io::Result<()> {
  let man = clap_mangen::Man::new(app);
  let mut man_buffer: Vec<u8> = Default::default();
  man.render(&mut man_buffer)?;
  let out_dir = std::env::current_dir()?;
  std::fs::write(
    out_dir.join(&format!("./target/man/{name}.1", name = name)),
    man_buffer,
  )?;

  Ok(())
}

pub fn generate_man() -> std::io::Result<()> {
  generate_man_command("nanocl", cli::Cli::into_app())?;
  generate_man_command("nanocl-namespace", cli::NamespaceArgs::into_app())?;
  generate_man_command(
    "nanocl-namespace-create",
    NamespacePartial::into_app(),
  )?;
  generate_man_command("nanocl-apply", cli::ApplyArgs::into_app())?;
  generate_man_command("nanocl-revert", cli::RevertArgs::into_app())?;
  Ok(())
}
