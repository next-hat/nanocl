use serde::Deserialize;
use clap::{Args, Parser};

use nanocld_client::stubs::{generic::GenericFilter, system::ObjPsStatus};

/// An empty filter to use as default
#[derive(Clone, Default, Args)]
pub struct GenericDefaultOpts;

/// A generic filter to use in the list operations
impl From<GenericDefaultOpts> for GenericFilter {
  fn from(_: GenericDefaultOpts) -> Self {
    Self::default()
  }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GenericProcessStatus {
  /// Status of the cargo
  pub status: ObjPsStatus,
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

/// Generic remove options for the remove command
#[derive(Clone, Parser)]
pub struct GenericRemoveOpts<T = GenericDefaultOpts>
where
  T: Args + Clone,
{
  /// The names of the objects to remove
  pub names: Vec<String>,
  #[clap(short = 'y', long)]
  pub skip_confirm: bool,
  /// Filters
  #[clap(flatten)]
  pub others: T,
}

/// Generic force options for the remove command
#[derive(Clone, Parser)]
pub struct GenericRemoveForceOpts {
  #[clap(short = 'f', long)]
  pub force: bool,
}

/// Generic start options for the start command
#[derive(Clone, Parser)]
pub struct GenericStartOpts {
  pub names: Vec<String>,
}

/// Generic stop options for the stop command
#[derive(Clone, Parser)]
pub struct GenericStopOpts {
  pub names: Vec<String>,
}
