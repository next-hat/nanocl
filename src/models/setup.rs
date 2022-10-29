use clap::Parser;

#[derive(Debug, Parser)]
pub struct SetupArgs {
  pub(crate) host: Option<String>,
}
