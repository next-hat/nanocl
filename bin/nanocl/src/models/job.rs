// use clap::{Parser, Subcommand};
// use nanocld_client::stubs::job::WaitCondition;

// /// ## Job wait options
// ///
// /// `nanocl job wait` available options
// ///
// #[derive(Parser)]
// pub struct JobWaitOpts {
//   /// Name of job to wait
//   pub name: String,
//   /// State to wait
//   #[clap(short = 'c')]
//   pub condition: Option<WaitCondition>,
// }

// /// ## Job list options
// ///
// /// `nanocl job ls` available options
// ///
// #[derive(Parser)]
// pub struct JobListOpts {}

// /// ## Job remove options
// ///
// /// `nanocl job rm` available options
// ///
// #[derive(Parser)]
// pub struct JobRemoveOpts {
//   /// Name of job to remove
//   pub names: Vec<String>,
// }

// #[derive(Parser)]
// pub struct JobInspectOpts {
//   /// Name of job to inspect
//   pub name: String,
// }

// #[derive(Parser)]
// pub struct JobLogsOpts {
//   /// Name of job to inspect
//   pub name: String,
// }

// /// ## JobCommand
// ///
// /// `nanocl cargo` available commands
// ///
// #[derive(Subcommand)]
// pub enum JobCommand {
//   /// List existing cargo
//   #[clap(alias("ls"))]
//   List(JobListOpts),
//   /// Remove cargo by its name
//   #[clap(alias("rm"))]
//   Remove(JobRemoveOpts),
//   /// Inspect a cargo by its name
//   Inspect(JobInspectOpts),
//   /// Show logs
//   Logs(JobLogsOpts),
//   /// Wait cargo
//   Wait(JobWaitOpts),
// }
