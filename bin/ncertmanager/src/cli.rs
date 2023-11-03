use clap::Parser;

const DEFAULT_CHECK_RENEW_DELAY: &str = "20";
// const DEFAULT_CHECK_RENEW_DELAY: &str = "86400";

#[derive(Parser)]
pub struct Cli {
  /// Interval in second to check certificate renew
  #[clap(long, short = 'i', default_value = DEFAULT_CHECK_RENEW_DELAY)]
  pub renew_interval: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    let args = Cli::parse_from(["nanocl-ncertmanager"]);
    assert_eq!(args.renew_interval, DEFAULT_CHECK_RENEW_DELAY);

    let args = Cli::parse_from(["nanocl-ncertmanager", "-i", "3600"]);
    assert_eq!(args.renew_interval, "3600");
  }
}
