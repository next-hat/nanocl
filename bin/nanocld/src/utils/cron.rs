use ntex::web;
use tokio::{fs, io::AsyncWriteExt};

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::job::Job;

use crate::{vars, models::SystemState};

/// Format the cron job command to start a job at a given time
fn format_cron_job_command(job: &Job, state: &SystemState) -> String {
  let host = state
    .config
    .hosts
    .first()
    .cloned()
    .unwrap_or("unix:///run/nanocl/nanocl.sock".to_owned())
    .replace("unix://", "");
  format!(
    "curl -X POST --unix {host} http://localhost/v{}/processes/job/{}/start",
    vars::VERSION,
    &job.name
  )
}

/// Execute the crontab command to update the cron jobs
async fn exec_crontab() -> IoResult<()> {
  web::block(|| {
    std::process::Command::new("crontab")
      .arg("/tmp/crontab")
      .output()
      .map_err(|err| err.map_err_context(|| "Cron job"))?;
    Ok::<_, IoError>(())
  })
  .await?;
  Ok(())
}

/// Add a cron rule to the crontab to start a job at a given time
pub async fn add_cron_rule(
  item: &Job,
  schedule: &str,
  state: &SystemState,
) -> IoResult<()> {
  let cmd = format_cron_job_command(item, state);
  let cron_rule = format!("{} {cmd}", schedule);
  log::debug!("Creating cron rule: {cron_rule}");
  fs::copy("/var/spool/cron/crontabs/root", "/tmp/crontab")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  let mut file = fs::OpenOptions::new()
    .write(true)
    .append(true)
    .open("/tmp/crontab")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  file
    .write_all(format!("{cron_rule}\n").as_bytes())
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  exec_crontab().await?;
  Ok(())
}

/// Remove a cron rule from the crontab for the given job
pub async fn remove_cron_rule(item: &Job, state: &SystemState) -> IoResult<()> {
  let mut content = fs::read_to_string("/var/spool/cron/crontabs/root")
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  let cmd = format_cron_job_command(item, state);
  log::debug!("Removing cron rule: {cmd}");
  content = content
    .lines()
    .filter(|line| !line.contains(&cmd))
    .collect::<Vec<_>>()
    .join("\n");
  fs::write("/tmp/crontab", format!("{content}\n"))
    .await
    .map_err(|err| err.map_err_context(|| "Cron job"))?;
  exec_crontab().await?;
  Ok(())
}
