/// ## JobWaitOpts
///
/// `nanocl cargo wait` available options
///
#[derive(Debug, Parser)]
pub struct JobWaitOpts {
  /// Name of cargo to wait
  pub name: String,
  /// State to wait
  #[clap(short = 'c')]
  pub condition: Option<WaitCondition>,
}

/// ## JobCommand
///
/// `nanocl cargo` available commands
///
#[derive(Debug, Subcommand)]
#[clap(about, version)]
pub enum JobCommand {
  /// List existing cargo
  #[clap(alias("ls"))]
  List(JobListOpts),
  /// Remove cargo by its name
  #[clap(alias("rm"))]
  Remove(JobRemoveOpts),
  /// Inspect a cargo by its name
  Inspect(JobInspectOpts),
  /// Show logs
  Logs(JobLogsOpts),
  /// Wait cargo
  Wait(JobWaitOpts),
}
