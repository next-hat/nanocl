use clap::*;
use std::fs;

include!("./src/cli.rs");

const MAN_PATH: &str = "../../target/man";

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
    out_dir.join(format!("{MAN_PATH}/{name}.1", name = name)),
    man_buffer,
  )?;

  Ok(())
}

fn main() -> std::io::Result<()> {
  fs::create_dir_all(MAN_PATH)?;
  generate_man_command("nanocld", Cli::command())?;
  Ok(())
}
