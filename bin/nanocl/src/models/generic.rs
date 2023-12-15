use clap::Parser;

use nanocld_client::stubs::generic::GenericFilter;

#[derive(Clone, Parser)]
pub struct GenericListOpts {
  /// Only show keys
  #[clap(long, short)]
  pub quiet: bool,
  /// Limit the number of results default to 100
  #[clap(long, short)]
  pub limit: Option<usize>,
  /// Offset the results to navigate through the results
  #[clap(long, short)]
  pub offset: Option<usize>,
}

impl From<GenericListOpts> for GenericFilter {
  fn from(opts: GenericListOpts) -> Self {
    Self {
      limit: opts.limit,
      offset: opts.offset,
      ..Default::default()
    }
  }
}
