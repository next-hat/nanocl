use clap::Parser;

#[derive(Clone, Parser)]
#[clap(name = "nanocl backup")]
pub struct BackupOpts {
  /// Directory where to write the backup default to the current directory
  #[clap(short, long)]
  pub output_dir: Option<String>,
  /// Skip confirmation
  #[clap(short = 'y', long = "yes")]
  pub skip_confirm: bool,
}
