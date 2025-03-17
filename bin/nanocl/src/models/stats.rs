use clap::Parser;

#[derive(Default, Parser)]
pub struct StatsOpts {
  pub names: Vec<String>,
}
