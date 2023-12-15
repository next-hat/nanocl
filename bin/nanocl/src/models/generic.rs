use clap::Parser;

#[derive(Clone, Parser)]
pub struct GenericListOpts {
  /// Only show keys
  #[clap(long, short)]
  pub quiet: bool,
  /// Limit the number of results default to 100
  #[clap(long, short)]
  pub limit: Option<i64>,
  /// Offset the results to navigate through the results
  #[clap(long, short)]
  pub offset: Option<i64>,
}
