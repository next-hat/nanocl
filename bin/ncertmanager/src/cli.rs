use clap::Parser;

#[derive(Parser)]
pub struct Cli {
  /// Path to nginx config directory
  #[clap(long)]
  pub conf_dir: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    // let args = Cli::parse_from(["nanocl-ncertmanager"]);
    // Cli::parse();
  }
}
