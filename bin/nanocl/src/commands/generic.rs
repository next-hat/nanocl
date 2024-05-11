use clap::Args;

use nanocld_client::{
  stubs::{
    generic::{GenericFilter, GenericListQuery},
    system::{EventActorKind, NativeEventAction},
  },
  NanocldClient,
};
use nanocl_error::io::{FromIo, IoResult};

use crate::{
  utils,
  models::{GenericRemoveOpts, GenericListOpts},
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
