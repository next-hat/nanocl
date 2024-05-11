use clap::Args;

use nanocld_client::{
  NanocldClient,
  stubs::generic::{GenericFilter, GenericListQuery},
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
        opts.names.join(",")
      ))
      .map_err(|err| err.map_err_context(|| "Delete"))?;
    }
    for name in &opts.names {
      client
        .send_delete(
          &format!("/{}/{name}", Self::object_name()),
          Self::get_query(opts),
        )
        .await?;
    }
    Ok(())
  }
}
