use clap::{Args, Parser};

use nanocld_client::stubs::generic::GenericFilter;

/// An empty filter to use as default
#[derive(Clone, Args)]
pub struct GenericDefaultOpts;

/// A generic filter to use in the list operations
impl From<GenericDefaultOpts> for GenericFilter {
  fn from(_: GenericDefaultOpts) -> Self {
    Self::default()
  }
}

/// Generic list options for the list command
#[derive(Clone, Parser)]
pub struct GenericListOpts<T = GenericDefaultOpts>
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

/// Convert the generic list options to a generic filter
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

/// Generic delete options for the delete command
#[derive(Clone, Parser)]
pub struct GenericDeleteOpts<T = GenericDefaultOpts>
where
  T: Args + Clone,
{
  /// The names of the objects to delete
  pub names: Vec<String>,
  #[clap(short = 'y')]
  pub skip_confirm: bool,
  /// Filters
  #[clap(flatten)]
  pub others: T,
}
