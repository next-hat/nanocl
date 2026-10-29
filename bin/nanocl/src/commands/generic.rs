use clap::Args;
use indicatif::{MultiProgress, ProgressBar};
use ntex::http::StatusCode;

use nanocl_error::{
  http_client::{HttpClientError, HttpClientResult},
  io::{FromIo, IoError, IoResult},
};
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericListQuery},
    system::{NativeEventAction, ObjPsStatus, ObjPsStatusKind},
  },
};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
  config::CliConfig,
  models::{
    GenericInspectOpts, GenericListOpts, GenericRemoveOpts, GenericStartOpts,
    GenericStopOpts, StateOutput,
  },
  utils,
};

/// A batch may continue after individual failures, but must fail as a command.
fn lifecycle_result(action: &str, failures: usize) -> IoResult<()> {
  if failures == 0 {
    Ok(())
  } else {
    Err(IoError::other(
      action,
      &format!("{failures} item(s) failed"),
    ))
  }
}

fn lifecycle_is_unchanged(
  object_name: &str,
  status: &ObjPsStatus,
  requested: &ObjPsStatusKind,
) -> bool {
  matches!(object_name, "cargoes" | "vms")
    && matches!(requested, ObjPsStatusKind::Start | ObjPsStatusKind::Stop)
    && status.wanted == *requested
    && status.actual == *requested
}

pub(super) async fn inspect_lifecycle_unchanged(
  client: &NanocldClient,
  object_name: &str,
  key: &str,
  requested: &ObjPsStatusKind,
) -> HttpClientResult<bool> {
  let status = match object_name {
    "cargoes" => client.inspect_cargo(key).await?.status,
    "vms" => client.inspect_vm(key).await?.status,
    _ => return Ok(false),
  };
  // Unchanged describes this observation; it does not acknowledge a new
  // operation or assert that another caller's operation has committed.
  Ok(lifecycle_is_unchanged(object_name, &status, requested))
}

pub trait GenericCommand {
  fn object_name() -> &'static str;
}

pub trait GenericCommandLs: GenericCommand {
  type Item;
  type CompactItem: tabled::Tabled + From<Self::Item>;
  type Args;
  type ApiItem;

  fn get_key(item: &Self::Item) -> String;

  fn render_list<T>(opts: &GenericListOpts<T>, rows: Vec<Self::Item>) -> String
  where
    Self::Item: tabled::Tabled,
    T: Args + Clone + Default,
  {
    if opts.quiet {
      return rows
        .iter()
        .map(Self::get_key)
        .collect::<Vec<_>>()
        .join("\n");
    }
    if opts.wide {
      utils::print::render_table(rows)
    } else {
      utils::print::render_table(rows.into_iter().map(Self::CompactItem::from))
    }
  }

  fn print_table<T>(opts: &GenericListOpts<T>, rows: Vec<Self::Item>)
  where
    Self::Item: tabled::Tabled,
    T: Args + Clone + Default,
  {
    let output = Self::render_list(opts, rows);
    if !output.is_empty() {
      println!("{output}");
    }
  }

  fn gen_default_filter<T>(
    _args: &Self::Args,
    opts: &GenericListOpts<T>,
  ) -> GenericFilter
  where
    T: Into<GenericFilter> + Args + Clone + Default,
  {
    let mut filter = opts.others.clone().unwrap_or_default().into();
    if let Some(limit) = opts.limit {
      filter = filter.limit(limit);
    }
    if let Some(offset) = opts.offset {
      filter = filter.offset(offset);
    }
    filter
  }

  fn transform_filter(
    _args: &Self::Args,
    filter: &GenericFilter,
    _namespace: Option<&str>,
  ) -> impl serde::Serialize {
    GenericListQuery::try_from(filter.clone()).unwrap()
  }

  async fn exec_ls<T>(
    client: &NanocldClient,
    args: &Self::Args,
    opts: &GenericListOpts<T>,
    namespace: Option<&str>,
  ) -> IoResult<()>
  where
    Self::ApiItem: serde::de::DeserializeOwned + Send + 'static,
    Self::Item: tabled::Tabled + From<Self::ApiItem>,
    T: Into<GenericFilter> + Args + Clone + Default,
  {
    let filter = Self::gen_default_filter(args, opts);
    let transform_filter = Self::transform_filter(args, &filter, namespace);
    let res = client
      .send_get(&format!("/{}", Self::object_name()), Some(transform_filter))
      .await?;
    let items = NanocldClient::res_json::<Vec<Self::ApiItem>>(res).await?;
    let rows = items
      .into_iter()
      .map(Self::Item::from)
      .collect::<Vec<Self::Item>>();
    Self::print_table(opts, rows);
    Ok(())
  }
}

pub trait GenericCommandRm<T, Q>: GenericCommand
where
  T: Args + Clone,
  Q: serde::Serialize,
{
  fn get_query(_opts: &GenericRemoveOpts<T>) -> Option<Q>
  where
    Q: serde::Serialize,
  {
    None
  }

  async fn exec_rm(
    client: &NanocldClient,
    opts: &GenericRemoveOpts<T>,
  ) -> IoResult<()> {
    Self::exec_rm_with_progress(client, opts, None, None)
      .await
      .and_then(|failures| lifecycle_result("Remove", failures))
  }

  async fn exec_rm_with_progress(
    client: &NanocldClient,
    opts: &GenericRemoveOpts<T>,
    progress: Option<(&MultiProgress, &ProgressBar)>,
    output: Option<&StateOutput>,
  ) -> IoResult<usize> {
    let object_name = Self::object_name();
    if !opts.skip_confirm {
      utils::dialog::confirm(&format!(
        "Delete {object_name} {} ?",
        opts.keys.join(",")
      ))
      .map_err(|err| err.map_err_context(|| "Delete"))?;
    }
    let mut failures = 0;
    for key in &opts.keys {
      let token = format!("{object_name}/{key}");
      let pg = match progress {
        Some((display, summary)) => {
          let kind = match object_name {
            "cargoes" => "cargo",
            "vms" => "vm",
            "jobs" => "job",
            "resources" => "resource",
            "secrets" => "secret",
            _ => object_name,
          };
          let token = format!("{kind}/{key}");
          let pg = utils::progress::create_state_item(
            display, summary, &token, output,
          )?;
          utils::progress::set_state_message(&pg, summary, "Removing", output)?;
          pg
        }
        None => {
          let style = utils::progress::create_spinner_style(&token, "red");
          utils::progress::create_progress("(destroying)", &style)
        }
      };
      let finish =
        |status: &str, failed, error: Option<&str>| -> IoResult<()> {
          if let Some((_, summary)) = progress {
            utils::progress::finish_state_item(
              &pg, summary, status, failed, error, output,
            )?;
          } else {
            pg.finish_with_message(format!("({})", status.to_lowercase()));
          }
          Ok(())
        };
      let deletion = if object_name == "jobs" {
        let job = match client.inspect_job(key).await {
          Ok(job) => job,
          Err(HttpClientError::HttpError(error))
            if error.status == StatusCode::NOT_FOUND =>
          {
            finish("Unchanged", false, None)?;
            continue;
          }
          Err(error) => {
            finish("Failed", true, Some(&error.to_string()))?;
            return Err(error.into());
          }
        };
        utils::process::wait_job_deletion(
          key,
          job.incarnation_key,
          job.current_run_key,
          client,
          client.delete_job_if_current(
            key,
            job.incarnation_key,
            job.current_run_key,
          ),
        )
        .await
      } else if matches!(object_name, "cargoes" | "vms") {
        let kind = utils::process::get_actor_kind(object_name);
        utils::process::wait_process_operation(
          key,
          kind.clone(),
          NativeEventAction::Destroy,
          client,
          client.delete_process_operation(
            &kind.to_string().to_lowercase(),
            key,
            Self::get_query(opts),
          ),
        )
        .await
      } else {
        client
          .send_delete(
            &format!("/{}/{key}", Self::object_name()),
            Self::get_query(opts),
          )
          .await
          .map(|_| ())
      };
      if let Err(err) = deletion {
        if let HttpClientError::HttpError(err) = &err
          && err.status == StatusCode::NOT_FOUND
        {
          finish("Unchanged", false, None)?;
          continue;
        }
        failures += 1;
        finish("Failed", true, Some(&err.to_string()))?;
        if object_name == "jobs" {
          return Err(err.into());
        }
        if let Some((display, _)) = progress {
          display.suspend(|| eprintln!("{key}: {err}"));
        } else {
          eprintln!("{key}: {err}");
        }
        continue;
      }
      finish("Destroyed", false, None)?;
    }
    Ok(failures)
  }
}

pub trait GenericCommandStart: GenericCommand {
  async fn exec_start(
    client: &NanocldClient,
    opts: &GenericStartOpts,
  ) -> IoResult<()> {
    Self::exec_start_with_progress(client, opts, None, None)
      .await
      .and_then(|failures| lifecycle_result("Start", failures))
  }

  async fn exec_start_with_progress(
    client: &NanocldClient,
    opts: &GenericStartOpts,
    progress: Option<(&MultiProgress, &ProgressBar)>,
    output: Option<&StateOutput>,
  ) -> IoResult<usize> {
    let object_name = Self::object_name();
    let process_kind = utils::process::get_actor_kind(object_name);
    let kind = process_kind.to_string().to_lowercase();
    let mut failures = 0;
    for key in &opts.keys {
      let pg = match progress {
        Some((display, summary)) => {
          let token = format!("{kind}/{key}");
          let pg = utils::progress::create_state_item(
            display, summary, &token, output,
          )?;
          utils::progress::set_state_message(&pg, summary, "Starting", output)?;
          pg
        }
        None => {
          let token = format!("{object_name}/{key}");
          let style = utils::progress::create_spinner_style(&token, "cyan");
          utils::progress::create_progress("(starting)", &style)
        }
      };
      let finish =
        |status: &str, failed, error: Option<&str>| -> IoResult<()> {
          if let Some((_, summary)) = progress {
            utils::progress::finish_state_item(
              &pg, summary, status, failed, error, output,
            )?;
          } else {
            pg.finish_with_message(format!("({})", status.to_lowercase()));
          }
          Ok(())
        };
      let unchanged = inspect_lifecycle_unchanged(
        client,
        object_name,
        key,
        &ObjPsStatusKind::Start,
      )
      .await;
      let result = if let Err(error) = unchanged {
        Err(error)
      } else if matches!(unchanged, Ok(true)) {
        finish("Unchanged", false, None)?;
        continue;
      } else if object_name == "jobs" {
        client.start_process(&kind, key).await
      } else {
        utils::process::wait_process_operation(
          key,
          process_kind.clone(),
          NativeEventAction::Start,
          client,
          client.start_process_operation(&kind, key),
        )
        .await
      };
      if let Err(err) = result {
        failures += 1;
        finish("Failed", true, Some(&err.to_string()))?;
        if let Some((display, _)) = progress {
          display.suspend(|| eprintln!("{key}: {err}"));
        } else {
          eprintln!("{key}: {err}");
        }
        continue;
      }
      finish(
        if object_name == "jobs" {
          "Accepted"
        } else {
          "Started"
        },
        false,
        None,
      )?;
    }
    Ok(failures)
  }
}

pub trait GenericCommandStop: GenericCommand {
  async fn exec_stop(
    client: &NanocldClient,
    opts: &GenericStopOpts,
  ) -> IoResult<()> {
    Self::exec_stop_with_progress(client, opts, None, None)
      .await
      .and_then(|failures| lifecycle_result("Stop", failures))
  }

  async fn exec_stop_with_progress(
    client: &NanocldClient,
    opts: &GenericStopOpts,
    progress: Option<(&MultiProgress, &ProgressBar)>,
    output: Option<&StateOutput>,
  ) -> IoResult<usize> {
    let object_name = Self::object_name();
    let process_kind = utils::process::get_actor_kind(object_name);
    let kind = process_kind.to_string().to_lowercase();
    let mut failures = 0;
    for key in &opts.keys {
      let pg = match progress {
        Some((display, summary)) => {
          let token = format!("{kind}/{key}");
          let pg = utils::progress::create_state_item(
            display, summary, &token, output,
          )?;
          utils::progress::set_state_message(&pg, summary, "Stopping", output)?;
          pg
        }
        None => {
          let token = format!("{object_name}/{key}");
          let style = utils::progress::create_spinner_style(&token, "cyan");
          utils::progress::create_progress("(stopping)", &style)
        }
      };
      let finish =
        |status: &str, failed, error: Option<&str>| -> IoResult<()> {
          if let Some((_, summary)) = progress {
            utils::progress::finish_state_item(
              &pg, summary, status, failed, error, output,
            )?;
          } else {
            pg.finish_with_message(format!("({})", status.to_lowercase()));
          }
          Ok(())
        };
      let unchanged = inspect_lifecycle_unchanged(
        client,
        object_name,
        key,
        &ObjPsStatusKind::Stop,
      )
      .await;
      let result = if let Err(error) = unchanged {
        Err(error)
      } else if matches!(unchanged, Ok(true)) {
        finish("Unchanged", false, None)?;
        continue;
      } else if object_name == "vms" {
        // This endpoint returns only after confirmed shutdown, unlike Job admission.
        client.stop_vm(key, &Default::default()).await
      } else if object_name == "jobs" {
        client.stop_process(&kind, key).await
      } else {
        utils::process::wait_process_operation(
          key,
          process_kind.clone(),
          NativeEventAction::Stop,
          client,
          client.stop_process_operation(&kind, key),
        )
        .await
      };
      if let Err(err) = result {
        failures += 1;
        finish("Failed", true, Some(&err.to_string()))?;
        if let Some((display, _)) = progress {
          display.suspend(|| eprintln!("{key}: {err}"));
        } else {
          eprintln!("{key}: {err}");
        }
        continue;
      }
      finish(
        if object_name == "jobs" {
          "Accepted"
        } else {
          "Stopped"
        },
        false,
        None,
      )?;
    }
    Ok(failures)
  }
}

pub trait GenericCommandInspect: GenericCommand {
  type ApiItem;

  async fn exec_inspect(
    cli_conf: &CliConfig,
    opts: &GenericInspectOpts,
  ) -> IoResult<()>
  where
    Self::ApiItem: Serialize + DeserializeOwned + Send + 'static,
  {
    let res = cli_conf
      .client
      .send_get(
        &format!("/{}/{}/inspect", Self::object_name(), opts.key),
        None::<String>,
      )
      .await?;
    let item = NanocldClient::res_json::<Self::ApiItem>(res).await?;
    let display = opts
      .display
      .clone()
      .unwrap_or(cli_conf.user_config.display_format.clone());
    utils::print::display_format(&display, item)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::GenericDefaultOpts;

  #[test]
  fn lifecycle_noop_requires_matching_wanted_and_actual_status() {
    for object_name in ["cargoes", "vms"] {
      for requested in [ObjPsStatusKind::Start, ObjPsStatusKind::Stop] {
        let settled = ObjPsStatus {
          wanted: requested.clone(),
          actual: requested.clone(),
          ..Default::default()
        };
        assert!(lifecycle_is_unchanged(object_name, &settled, &requested));
        for other in [
          ObjPsStatusKind::Create,
          ObjPsStatusKind::Starting,
          ObjPsStatusKind::Start,
          ObjPsStatusKind::Updating,
          ObjPsStatusKind::Update,
          ObjPsStatusKind::Destroying,
          ObjPsStatusKind::Destroy,
          ObjPsStatusKind::Stopping,
          ObjPsStatusKind::Stop,
          ObjPsStatusKind::Fail,
          ObjPsStatusKind::Finish,
          ObjPsStatusKind::Unknown,
        ] {
          if other == requested {
            continue;
          }
          let different_wanted = ObjPsStatus {
            wanted: other.clone(),
            ..settled.clone()
          };
          let different_actual = ObjPsStatus {
            actual: other,
            ..settled.clone()
          };
          assert!(!lifecycle_is_unchanged(
            object_name,
            &different_wanted,
            &requested,
          ));
          assert!(!lifecycle_is_unchanged(
            object_name,
            &different_actual,
            &requested,
          ));
        }
      }
    }
  }

  #[test]
  fn lifecycle_noop_excludes_jobs_and_nonterminal_requests() {
    for requested in [ObjPsStatusKind::Start, ObjPsStatusKind::Stop] {
      let status = ObjPsStatus {
        wanted: requested.clone(),
        actual: requested.clone(),
        ..Default::default()
      };
      assert!(!lifecycle_is_unchanged("jobs", &status, &requested));
      assert!(!lifecycle_is_unchanged("resources", &status, &requested));
    }
    for requested in [ObjPsStatusKind::Starting, ObjPsStatusKind::Unknown] {
      let status = ObjPsStatus {
        wanted: requested.clone(),
        actual: requested.clone(),
        ..Default::default()
      };
      for object_name in ["cargoes", "vms"] {
        assert!(!lifecycle_is_unchanged(object_name, &status, &requested));
      }
    }
  }

  struct FailedBatch;

  impl GenericCommand for FailedBatch {
    fn object_name() -> &'static str {
      "fixture"
    }
  }

  impl GenericCommandStart for FailedBatch {
    async fn exec_start_with_progress(
      _: &NanocldClient,
      opts: &GenericStartOpts,
      _: Option<(&MultiProgress, &ProgressBar)>,
      _: Option<&StateOutput>,
    ) -> IoResult<usize> {
      Ok(opts.keys.len())
    }
  }

  impl GenericCommandStop for FailedBatch {
    async fn exec_stop_with_progress(
      _: &NanocldClient,
      opts: &GenericStopOpts,
      _: Option<(&MultiProgress, &ProgressBar)>,
      _: Option<&StateOutput>,
    ) -> IoResult<usize> {
      Ok(opts.keys.len())
    }
  }

  impl GenericCommandRm<GenericDefaultOpts, String> for FailedBatch {
    async fn exec_rm_with_progress(
      _: &NanocldClient,
      opts: &GenericRemoveOpts<GenericDefaultOpts>,
      _: Option<(&MultiProgress, &ProgressBar)>,
      _: Option<&StateOutput>,
    ) -> IoResult<usize> {
      Ok(opts.keys.len())
    }
  }

  // The fixture overrides all I/O paths: no server, socket, or daemon is used.
  #[ntex::test]
  async fn lifecycle_commands_propagate_batch_failures() {
    let client =
      NanocldClient::connect_to(&nanocld_client::ConnectOpts::default())
        .unwrap();
    for keys in [vec![], vec!["one".to_owned(), "two".to_owned()]] {
      let start = FailedBatch::exec_start(
        &client,
        &GenericStartOpts { keys: keys.clone() },
      )
      .await;
      let stop = FailedBatch::exec_stop(
        &client,
        &GenericStopOpts { keys: keys.clone() },
      )
      .await;
      let remove = FailedBatch::exec_rm(
        &client,
        &GenericRemoveOpts {
          keys: keys.clone(),
          skip_confirm: true,
          others: GenericDefaultOpts,
        },
      )
      .await;
      for result in [start, stop, remove] {
        if keys.is_empty() {
          result.unwrap();
        } else {
          assert!(result.unwrap_err().to_string().contains("2 item(s) failed"));
        }
      }
    }
  }
}
