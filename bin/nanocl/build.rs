use std::fs;
use clap::*;

include!("./src/models/mod.rs");

/// Man page name and command to generate
struct ManPage<'a> {
  name: &'a str,
  command: clap::Command,
}

/// Path where to generate the files
const MAN_PATH: &str = "../../target/man";

/// Function to generate a man page
fn generate_man_page<'a>(
  name: &'a str,
  app: &'a clap::Command,
) -> std::io::Result<()> {
  let man = clap_mangen::Man::new(app.to_owned());
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

pub fn generate_man_pages() -> std::io::Result<()> {
  let man_pages: Vec<ManPage> = vec![
    ManPage {
      name: "nanocl",
      command: Cli::command(),
    },
    ManPage {
      name: "nanocl-namespace",
      command: NamespaceArgs::command(),
    },
    ManPage {
      name: "nanocl-namespace-create",
      command: NamespaceOpts::command(),
    },
  ];

  fs::create_dir_all(MAN_PATH)?;
  for page in man_pages {
    generate_man_page(page.name, &page.command)?;
  }
  Ok(())
}

fn main() -> std::io::Result<()> {
  generate_man_pages()?;
  Ok(())
}
