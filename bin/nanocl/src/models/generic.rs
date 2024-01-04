use clap::{Args, Parser};

use nanocld_client::stubs::generic::GenericFilter;

#[derive(Clone, Args)]
pub struct DefaultFilter;

impl From<DefaultFilter> for GenericFilter {
  fn from(_: DefaultFilter) -> Self {
    Self::default()
  }
}

#[derive(Clone, Parser)]
pub struct GenericListOpts<T = DefaultFilter>
where
  T: Args + Clone,
{
  /// Only show keys
  #[clap(long, short)]
  pub quiet: bool,
  /// Limit the number of results default to 100
  #[clap(long, short)]
  pub limit: Option<usize>,
  /// Offset the results to navigate through the results
  #[clap(long, short)]
  pub offset: Option<usize>,
  /// Filters
  #[clap(long)]
  pub filters: Option<Vec<String>>,
  #[clap(flatten)]
  pub others: Option<T>,
}

impl<T> From<GenericListOpts<T>> for GenericFilter
where
  T: Args + Clone,
{
  fn from(opts: GenericListOpts<T>) -> Self {
    Self {
      limit: opts.limit,
      offset: opts.offset,
      ..Default::default()
    }
  }
}
