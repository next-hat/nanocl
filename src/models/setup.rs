use clap::Parser;

/// Download and Install required dependencies
#[derive(Debug, Parser)]
pub struct SetupArgs {
  /// Remote host to setup nanocl
  pub(crate) host: Option<String>,
}
