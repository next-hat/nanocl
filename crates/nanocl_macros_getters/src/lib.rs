use proc_macro2::TokenStream;
use syn::DeriveInput;
use quote::quote;
use syn::{Data, DataStruct, Fields};

pub fn expand_getters(input: DeriveInput) -> TokenStream {
  // let fields = match input.data {
  //   Data::Struct(DataStruct {
  //     fields: Fields::Named(fields),
  //     ..
  //   }) => fields.named,
  //   _ => panic!("this derive macro only works on structs with named fields"),
  // };
  let st_name = input.ident;

  let find_by_key_quote = quote! {
    pub async fn find_by_key(key: &str, pool: &Pool) -> IoResult<CargoDbModel> {
      use crate::schema::cargoes::dsl;
      let key = key.to_owned();
      let pool = pool.clone();
      let item = web::block(move || {
        let mut conn = utils::store::get_pool_conn(&pool)?;
        let item = dsl::cargoes
          .filter(dsl::key.eq(key))
          .get_result(&mut conn)
          .map_err(|err| err.map_err_context(|| "Cargo"))?;
        Ok(item)
      })
      .await?;
      Ok(item)
    }
  };

  quote! {
    #[automatically_derived]
    impl #st_name {
      #find_by_key_quote
    }
  }
}

#[macro_export]
macro_rules! repository_list_by {
  ( $schema:ident, $field_name:ident, $query:expr, $pool: expr, $err_ctx: expr ) => {{
    let query = $query.clone();
    let pool = $pool.clone();
    let items = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let mut sql = CargoDbModel::belonging_to(&query.namespace).into_boxed();
      if let Some(filter) = &query.$field_name {
        sql = sql.filter($schema::$field_name.ilike(format!("%{filter}%")));
      }
      if let Some(limit) = query.limit {
        sql = sql.limit(limit);
      }
      if let Some(offset) = query.offset {
        sql = sql.offset(offset);
      }
      let items = sql
        .order($schema::created_at.desc())
        .get_results(&mut conn)
        .map_err(|err| err.map_err_context(|| $err_ctx))?;
      Ok(items)
    })
    .await?;

    items
  }};
}

#[macro_export]
macro_rules! repository_delete_by {
  ( $table:expr, $field:expr, $filter:expr, $pool: expr, $err_ctx: expr ) => {{
    let filter = $filter.to_owned();
    let pool = $pool.clone();
    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = diesel::delete($table)
        .filter($field.eq(filter))
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(|| $err_ctx))?;
      Ok(item)
    })
    .await?;

    item
  }};
}

#[macro_export]
macro_rules! repository_delete_by_id {
  ( $table:path, $pk:ident, $pool: ident, $err_ctx: literal ) => {{
    let pk = $pk.to_owned();
    let pool = $pool.clone();
    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = diesel::delete($table.find(pk))
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(|| $err_ctx))?;
      Ok(item)
    })
    .await?;

    item
  }};
}

#[macro_export]
macro_rules! repository_find_by {
  ( $table:path, $field:path, $filter:ident, $pool: ident, $err_ctx:literal ) => {{
    let filter = $filter.to_owned();
    let pool = $pool.clone();
    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = $table
        .filter($field.eq(filter))
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| $err_ctx))?;
      Ok(item)
    })
    .await?;

    item
  }};
}

#[macro_export]
macro_rules! repository_find_by_id {
  ( $table:path, $pk:ident, $pool: ident, $err_ctx:literal ) => {{
    let pk = $pk.to_owned();
    let pool = $pool.clone();
    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = $table
        .find(pk)
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| $err_ctx))?;
      Ok(item)
    })
    .await?;

    item
  }};
}

#[macro_export]
macro_rules! repository_update_by_id {
  ( $table:path, $pk:ident, $new_item: ident, $pool: ident, $err_ctx:literal ) => {{
    let pk = $pk.to_owned();
    let pool = $pool.clone();

    web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      diesel::update($table.find(pk))
        .set(&$new_item)
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?;
      Ok(())
    })
    .await?;
  }};
}

#[macro_export]
macro_rules! repository_update_by_id_with_res {
  ( $table:path, $pk:ident, $new_item: ident, $pool: ident, $err_ctx:literal ) => {{
    let pk = $pk.to_owned();
    let pool = $pool.clone();

    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let item = diesel::update($table.find(pk))
        .set(&$new_item)
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?;
      Ok(item)
    })
    .await?;

    item
  }};
}

#[macro_export]
macro_rules! repository_create {
  ( $table:path, $new_item: ident, $pool: ident, $err_ctx:literal ) => {{
    let pool = $pool.clone();

    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = diesel::insert_into($table)
        .values(&$new_item)
        .execute(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?;
      Ok($new_item)
    })
    .await?;

    item
  }};
}
#[macro_export]
macro_rules! repository_create_with_res {
  ( $table:path, $new_item: ident, $pool: ident, $err_ctx:literal ) => {{
    let pool = $pool.clone();

    let item = web::block(move || {
      let mut conn = utils::store::get_pool_conn(&pool)?;
      let res = diesel::insert_into($table)
        .values(&$new_item)
        .get_result(&mut conn)
        .map_err(|err| err.map_err_context(|| "Cargo"))?;
      Ok(res)
    })
    .await?;

    item
  }};
}
