use clap::Args;
use ntex::http::StatusCode;

use nanocld_client::{
  stubs::{
    generic::{GenericFilter, GenericListQuery, GenericNspQuery},
    system::{EventActorKind, NativeEventAction, ObjPsStatusKind},
  },
  NanocldClient,
};
use nanocl_error::{
  io::{FromIo, IoResult},
  http_client::HttpClientError,
};
use serde::{de::DeserializeOwned, Serialize};

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
  ) -> impl serde::Serialize {
    GenericListQuery::try_from(filter.clone()).unwrap()
  }

  async fn exec_ls<T>(
    client: &NanocldClient,
    args: &Self::Args,
    opts: &GenericListOpts<T>,
  ) -> IoResult<()>
  where
    Self::ApiItem: serde::de::DeserializeOwned + Send + 'static,
    Self::Item: tabled::Tabled + From<Self::ApiItem>,
    T: Into<GenericFilter> + Args + Clone + Default,
  {
    let filter = Self::gen_default_filter(args, opts);
    let transform_filter = Self::transform_filter(args, &filter);
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
  fn get_query(
    _opts: &GenericRemoveOpts<T>,
    _namespace: Option<String>,
  ) -> Option<Q>
  where
    Q: serde::Serialize,
  {
    None
  }

  async fn exec_rm(
    client: &NanocldClient,
    opts: &GenericRemoveOpts<T>,
    namespace: Option<String>,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    if !opts.skip_confirm {
      utils::dialog::confirm(&format!(
        "Delete {object_name} {} ?",
        opts.keys.join(",")
      ))
      .map_err(|err| err.map_err_context(|| "Delete"))?;
    }
    let pg_style = utils::progress::create_spinner_style("red");
    for name in &opts.keys {
      let token = format!("{object_name}/{}", name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      let (key, waiter_kind) = match object_name {
        "vms" => (
          format!("{name}.{}", namespace.clone().unwrap_or_default()),
          Some(EventActorKind::Vm),
        ),
        "cargoes" => (
          format!("{name}.{}", namespace.clone().unwrap_or_default()),
          Some(EventActorKind::Cargo),
        ),
        "jobs" => (name.clone(), Some(EventActorKind::Job)),
        _ => (name.clone(), None),
      };
      let waiter = match waiter_kind {
        Some(kind) => {
          let waiter = utils::process::wait_process_state(
            &key,
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
          &format!("/{}/{name}", Self::object_name()),
          Self::get_query(opts, namespace.clone()),
        )
        .await
      {
        if let HttpClientError::HttpError(err) = &err {
          if err.status == StatusCode::NOT_FOUND {
            pg.finish();
            continue;
          }
        }
        pg.finish_and_clear();
        eprintln!("{err} {name}");
        continue;
      }
      if let Some(waiter) = waiter {
        waiter.await??;
      }
      pg.finish();
    }
    Ok(())
  }
}

pub trait GenericCommandStart: GenericCommand {
  async fn exec_start(
    client: &NanocldClient,
    opts: &GenericStartOpts,
    namespace: Option<String>,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    for name in &opts.names {
      let status = utils::process::get_process_status(
        object_name,
        name,
        namespace.clone(),
        client,
      )
      .await?;
      if status.actual == ObjPsStatusKind::Start {
        eprintln!("{name} is already started");
        continue;
      }
      let key = utils::process::gen_key(name, namespace.clone());
      let process_kind = utils::process::get_actor_kind(object_name);
      let waiter = utils::process::wait_process_state(
        &key,
        process_kind.clone(),
        [NativeEventAction::Start].to_vec(),
        client,
      )
      .await?;
      if let Err(err) = client
        .start_process(
          process_kind.to_string().to_lowercase().as_str(),
          name,
          namespace.as_deref(),
        )
        .await
      {
        eprintln!("{err} {name}");
        continue;
      };
      if let Err(err) = waiter.await? {
        eprintln!("{err} {name}");
      }
    }
    Ok(())
  }
}

pub trait GenericCommandStop: GenericCommand {
  async fn exec_stop(
    client: &NanocldClient,
    opts: &GenericStopOpts,
    namespace: Option<String>,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    for name in &opts.names {
      let status = utils::process::get_process_status(
        object_name,
        name,
        namespace.clone(),
        client,
      )
      .await?;
      if status.actual == ObjPsStatusKind::Stop {
        eprintln!("{name} is already stopped");
        continue;
      }
      let key = utils::process::gen_key(name, namespace.clone());
      let process_kind = utils::process::get_actor_kind(object_name);
      let waiter = utils::process::wait_process_state(
        &key,
        process_kind.clone(),
        [NativeEventAction::Stop].to_vec(),
        client,
      )
      .await?;
      if let Err(err) = client
        .stop_process(
          process_kind.to_string().to_lowercase().as_str(),
          name,
          namespace.as_deref(),
        )
        .await
      {
        eprintln!("{err} {name}");
        continue;
      }
      if let Err(err) = waiter.await? {
        eprintln!("{err} {name}");
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
    namespace: Option<String>,
  ) -> IoResult<()>
  where
    Self::ApiItem: Serialize + DeserializeOwned + Send + 'static,
  {
    let res = cli_conf
      .client
      .send_get(
        &format!("/{}/{}/inspect", Self::object_name(), opts.key),
        Some(GenericNspQuery::new(namespace.as_deref())),
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
