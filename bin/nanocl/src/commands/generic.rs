use clap::Args;
use ntex::http::StatusCode;

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericListQuery, GenericNspQuery},
    system::{EventActorKind, NativeEventAction, ObjPsStatusKind},
  },
};
use nanocl_error::{
  io::{FromIo, IoResult},
  http_client::HttpClientError,
};

use crate::{
  utils,
  models::{
    GenericListOpts, GenericProcessStatus, GenericRemoveOpts, GenericStartOpts,
  },
};

pub trait GenericList {
  type Item;
  type Args;
  type ApiItem;

  fn object_name() -> &'static str;

  fn get_key(item: &Self::Item) -> String;

  fn print_table<T>(opts: &GenericListOpts<T>, rows: Vec<Self::Item>)
  where
    Self::Item: tabled::Tabled,
    T: Args + Clone,
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
    T: Into<GenericFilter> + Args + Clone,
  {
    let mut filter = if let Some(f) = &opts.others {
      f.clone().into()
    } else {
      GenericFilter::new()
    };
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
    T: Into<GenericFilter> + Args + Clone,
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

pub trait GenericRemove<T, Q>
where
  T: Args + Clone,
  Q: serde::Serialize,
{
  fn object_name() -> &'static str;

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
        opts.names.join(",")
      ))
      .map_err(|err| err.map_err_context(|| "Delete"))?;
    }
    let pg_style = utils::progress::create_spinner_style("red");
    for name in &opts.names {
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

pub trait GenericStart {
  fn object_name() -> &'static str;

  async fn exec_start(
    client: &NanocldClient,
    opts: &GenericStartOpts,
    namespace: Option<String>,
  ) -> IoResult<()> {
    let object_name = Self::object_name();
    for name in &opts.names {
      let res = client
        .send_get(
          &format!("/{object_name}/{name}/inspect"),
          Some(GenericNspQuery::new(namespace.as_deref())),
        )
        .await?;
      let status = NanocldClient::res_json::<GenericProcessStatus>(res)
        .await?
        .status;
      if status.actual == ObjPsStatusKind::Start {
        eprintln!("{} is already started", name);
        continue;
      }
      let key = match namespace {
        Some(ref namespace) => format!("{name}.{namespace}"),
        None => name.clone(),
      };
      let process_kind = match object_name {
        "vms" => EventActorKind::Vm,
        "cargoes" => EventActorKind::Cargo,
        "jobs" => EventActorKind::Job,
        _ => panic!(
          "The developer trolled you with a wrong object name {object_name}"
        ),
      };
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
