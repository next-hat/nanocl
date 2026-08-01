use clap::Args;
use ntex::http::StatusCode;

use nanocl_error::{
  http_client::HttpClientError,
  io::{FromIo, IoError, IoResult},
};
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericListQuery},
    system::{EventActorKind, NativeEventAction, ObjPsStatusKind},
  },
};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
  config::CliConfig,
  models::{
    GenericInspectOpts, GenericListOpts, GenericRemoveOpts, GenericStartOpts,
    GenericStopOpts,
  },
  utils,
};

pub trait GenericCommand {
  fn object_name() -> &'static str;
}

pub trait GenericCommandLs: GenericCommand {
  type Item;
  type Args;
  type ApiItem;

  fn get_key(item: &Self::Item) -> String;

  fn print_table<T>(opts: &GenericListOpts<T>, rows: Vec<Self::Item>)
  where
    Self::Item: tabled::Tabled,
    T: Args + Clone + Default,
  {
    match opts.quiet {
      true => {
        for row in rows {
          println!("{}", Self::get_key(&row));
        }
      }
      false => {
        utils::print::print_table(rows);
      }
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
    let object_name = Self::object_name();
    if !opts.skip_confirm {
      utils::dialog::confirm(&format!(
        "Delete {object_name} {} ?",
        opts.keys.join(",")
      ))
      .map_err(|err| err.map_err_context(|| "Delete"))?;
    }
    for key in &opts.keys {
      let token = format!("{object_name}/{key}");
      let pg_style = utils::progress::create_spinner_style(&token, "red");
      let pg = utils::progress::create_progress("(destroying)", &pg_style);
      let waiter_kind = match object_name {
        "vms" => Some(EventActorKind::Vm),
        "cargoes" => Some(EventActorKind::Cargo),
        "jobs" => Some(EventActorKind::Job),
        _ => None,
      };
      let waiter = match waiter_kind {
        Some(kind) => {
          let waiter = utils::process::wait_process_state(
            key,
            kind,
            vec![NativeEventAction::Destroy],
            client,
          )
          .await?;
          Some(waiter)
        }
        None => None,
      };
      if let Err(err) = client
        .send_delete(
          &format!("/{}/{key}", Self::object_name()),
          Self::get_query(opts),
        )
        .await
      {
        if let HttpClientError::HttpError(err) = &err
          && err.status == StatusCode::NOT_FOUND
        {
          pg.finish_with_message("(unchanged)");
          continue;
        }
        pg.finish();
        eprintln!("{key}: {err}");
        continue;
      }
      if let Some(waiter) = waiter {
        waiter.await.map_err(|err| {
          IoError::interrupted("wait_process_state", &err.to_string())
        })??;
      }
      pg.finish_with_message("(destroyed)");
    }
    Ok(())
  }
}

pub trait GenericCommandStart: GenericCommand {
  async fn exec_start(
    client: &NanocldClient,
    opts: &GenericStartOpts,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    for key in &opts.keys {
      let status =
        utils::process::get_process_status(object_name, key, client).await?;
      if status.actual == ObjPsStatusKind::Start {
        eprintln!("{key} is already started");
        continue;
      }
      let process_kind = utils::process::get_actor_kind(object_name);
      let waiter = utils::process::wait_process_state(
        key,
        process_kind.clone(),
        [NativeEventAction::Start].to_vec(),
        client,
      )
      .await?;
      if let Err(err) = client
        .start_process(process_kind.to_string().to_lowercase().as_str(), key)
        .await
      {
        eprintln!("{err} {key}");
        continue;
      };
      if let Err(err) = waiter.await.map_err(|err| {
        IoError::interrupted("wait_process_state", &err.to_string())
      })? {
        eprintln!("{err} {key}");
      }
    }
    Ok(())
  }
}

pub trait GenericCommandStop: GenericCommand {
  async fn exec_stop(
    client: &NanocldClient,
    opts: &GenericStopOpts,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    for key in &opts.keys {
      let status =
        utils::process::get_process_status(object_name, key, client).await?;
      if status.actual == ObjPsStatusKind::Stop {
        eprintln!("{key} is already stopped");
        continue;
      }
      let process_kind = utils::process::get_actor_kind(object_name);
      let waiter = utils::process::wait_process_state(
        key,
        process_kind.clone(),
        [NativeEventAction::Stop].to_vec(),
        client,
      )
      .await?;
      if let Err(err) = client
        .stop_process(process_kind.to_string().to_lowercase().as_str(), key)
        .await
      {
        eprintln!("{err} {key}");
        continue;
      }
      if let Err(err) = waiter.await.map_err(|err| {
        IoError::interrupted("wait_process_state", &err.to_string())
      })? {
        eprintln!("{err} {key}");
      }
    }
    Ok(())
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
