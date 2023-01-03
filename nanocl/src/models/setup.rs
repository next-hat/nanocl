use clap::Parser;

/// Setup given host to run nanocl
#[derive(Debug, Parser)]
pub struct SetupArgs {
  /// Remote host to setup nanocl
  pub(crate) host: Option<String>,
}
