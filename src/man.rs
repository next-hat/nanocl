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
    out_dir.join(&format!("../target/man/{name}.1", name = name)),
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
  // let man = clap_mangen::Man::new(cli::Cli::into_app());
  // let man_namespace = clap_mangen::Man::new(cli::NamespaceArgs::into_app());
  // let man_cluster = clap_mangen::Man::new(cli::ClusterArgs::into_app());
  // let mut man_buffer: Vec<u8> = Default::default();
  // man.render(&mut man_buffer)?;
  // let mut man_namespace_buffer: Vec<u8> = Default::default();
  // man_namespace.render(&mut man_namespace_buffer)?;

  // let out_dir = std::env::current_dir()?;
  // std::fs::write(out_dir.join("../target/man/nanocl.1"), man_buffer)?;
  // std::fs::write(out_dir.join("../target/man/nanocl.1"), man_namespace_buffer)?;
  Ok(())
}
