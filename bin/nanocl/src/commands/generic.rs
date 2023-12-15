use ntex::rt::JoinHandle;

use nanocld_client::{
  NanocldClient,
  stubs::generic::{GenericFilter, GenericListNspQuery},
};
use nanocl_error::io::{IoError, IoResult};

use crate::{utils, models::GenericListOpts};

pub trait GenericList {
  type Item;
  type Args;
  type ApiItem;

  fn object_name() -> &'static str;

  fn get_key(item: &Self::Item) -> String;

  fn print_table(opts: &GenericListOpts, rows: Vec<Self::Item>)
  where
    Self::Item: tabled::Tabled,
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

  fn get_list_query(
    _args: &Self::Args,
    opts: &GenericListOpts,
  ) -> GenericListNspQuery {
    GenericListNspQuery::try_from(GenericFilter::from(opts.clone())).unwrap()
  }

  fn exec_ls(
    client: &NanocldClient,
    args: &Self::Args,
    opts: &GenericListOpts,
  ) -> JoinHandle<IoResult<()>>
  where
    Self::Args: Clone + Send + 'static,
    Self::ApiItem: serde::de::DeserializeOwned + Send + 'static,
    Self::Item: tabled::Tabled + From<Self::ApiItem>,
  {
    let client = client.clone();
    let args = args.clone();
    let opts = opts.clone();
    ntex::rt::spawn(async move {
      let res = client
        .send_get(
          &format!("/{}", Self::object_name()),
          Some(Self::get_list_query(&args, &opts)),
        )
        .await?;
      let items = NanocldClient::res_json::<Vec<Self::ApiItem>>(res).await?;
      let rows = items
        .into_iter()
        .map(Self::Item::from)
        .collect::<Vec<Self::Item>>();
      Self::print_table(&opts, rows);
      Ok::<_, IoError>(())
    })
  }
}
