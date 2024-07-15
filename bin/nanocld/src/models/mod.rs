use diesel::r2d2::{ConnectionManager, Pool as R2D2Pool, PooledConnection};
use diesel::PgConnection;

mod ws;
use nanocl_error::io::{IoError, IoResult};
pub use ws::*;

mod node;
pub use node::*;

mod system;
pub use system::*;

mod metric;
pub use metric::*;

mod namespace;
pub use namespace::*;

mod cargo;
pub use cargo::*;

pub mod vm;
pub use vm::*;

pub mod vm_image;
pub use vm_image::*;

mod resource;
pub use resource::*;

mod resource_kind;
pub use resource_kind::*;

mod secret;
pub use secret::*;

mod job;
pub use job::*;

mod spec;
pub use spec::*;

mod process;
pub use process::*;

mod event;
pub use event::*;

mod raw_emitter;
pub use raw_emitter::*;

mod task_manager;
pub use task_manager::*;

mod object_process_status;
pub use object_process_status::*;

pub type Pool = R2D2Pool<ConnectionManager<PgConnection>>;
pub type DBConn = PooledConnection<ConnectionManager<PgConnection>>;

pub enum ColumnType {
  Text,
  Json,
  Uuid,
  // TODO: Implement Inet type
  // Inet,
  Timestamptz,
}

/// Generate a where clause for a json column
#[macro_export]
macro_rules! gen_sql_and4json {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        Box::new($query.and($column.is_null()))
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        Box::new($query.and($column.is_not_null()))
      }
      nanocl_stubs::generic::GenericClause::Contains(val) => {
        Box::new($query.and($column.contains(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::HasKey(val) => {
        Box::new($query.and($column.has_key(val.clone())))
      }
      _ => {
        panic!("Unsupported clause");
      }
    }
  };
}

// /// Generate clause for a string column
#[macro_export]
macro_rules! gen_sql_and4string {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        Box::new($query.and($column.eq(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Ne(val) => {
        Box::new($query.and($column.ne(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Gt(val) => {
        Box::new($query.and($column.gt(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Lt(val) => {
        Box::new($query.and($column.lt(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Ge(val) => {
        Box::new($query.and($column.ge(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Le(val) => {
        Box::new($query.and($column.le(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Like(val) => {
        Box::new($query.and($column.like(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::NotLike(val) => {
        Box::new($query.and($column.not_like(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::In(items) => {
        Box::new($query.and($column.eq_any(items.clone())))
      }
      nanocl_stubs::generic::GenericClause::NotIn(items) => {
        Box::new($query.and($column.ne_all(items.clone())))
      }
      nanocl_stubs::generic::GenericClause::IsNull => {
        Box::new($query.and($column.is_null()))
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        Box::new($query.and($column.is_not_null()))
      }
      _ => {
        panic!("Unsupported clause");
      }
    }
  };
}

/// Generate a where clause for a string column
#[macro_export]
macro_rules! gen_sql_where4string {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        $query = $query.filter($column.eq(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ne(val) => {
        $query = $query.filter($column.ne(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Gt(val) => {
        $query = $query.filter($column.gt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Lt(val) => {
        $query = $query.filter($column.lt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ge(val) => {
        $query = $query.filter($column.ge(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Le(val) => {
        $query = $query.filter($column.le(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Like(val) => {
        $query = $query.filter($column.like(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::NotLike(val) => {
        $query = $query.filter($column.not_like(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::In(items) => {
        $query = $query.filter($column.eq_any(items.clone()));
      }
      nanocl_stubs::generic::GenericClause::NotIn(items) => {
        $query = $query.filter($column.ne_all(items.clone()));
      }
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

pub fn parse_date_string(
  date_str: &str,
) -> IoResult<chrono::DateTime<chrono::Utc>> {
  // Define possible date and datetime formats
  let formats = [
    "%Y-%m-%d %H:%M:%S%.f %:z", // 2023-06-01 12:34:56.789 +02:00
    "%Y-%m-%d %H:%M:%S %:z",    // 2023-06-01 12:34:56 +02:00
    "%Y-%m-%d %H:%M:%S%.f",     // 2023-06-01 12:34:56.789
    "%Y-%m-%d %H:%M:%S",        // 2023-06-01 12:34:56
    "%Y-%m-%d",                 // 2023-06-01
  ];

  // Attempt to parse with each format
  for &format in &formats {
    if let Ok(datetime) = chrono::DateTime::parse_from_str(date_str, format) {
      // Convert to Utc if parsed as FixedOffset
      return Ok(datetime.with_timezone(&chrono::Utc));
    }
  }

  // Try parsing as RFC 3339 if other formats fail
  if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(date_str) {
    return Ok(datetime.with_timezone(&chrono::Utc));
  }

  // As a last resort, parse as NaiveDate (date without time) and assume UTC
  if let Ok(naive_date) =
    chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
  {
    let naive_datetime = chrono::NaiveDateTime::new(
      naive_date,
      chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    );
    return Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
      naive_datetime,
      chrono::Utc,
    ));
  }

  // If all formats fail, return error
  Err(IoError::invalid_data(
    "Invalid date format",
    &format!("Invalid date format: {}", date_str),
  ))
}

#[macro_export]
macro_rules! gen_sql_where4timestamptz {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.eq(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ne(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.ne(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Gt(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.gt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Lt(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.lt(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Ge(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.ge(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::Le(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        $query = $query.filter($column.le(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      nanocl_stubs::generic::GenericClause::In(items) => {
        let items: Vec<chrono::DateTime<chrono::Utc>> = items
          .iter()
          .map(|item| $crate::models::parse_date_string(item).unwrap())
          .collect();
        $query = $query.filter($column.eq_any(items));
      }
      nanocl_stubs::generic::GenericClause::NotIn(items) => {
        let items: Vec<chrono::DateTime<chrono::Utc>> = items
          .iter()
          .map(|item| $crate::models::parse_date_string(item).unwrap())
          .collect();
        $query = $query.filter($column.ne_all(items));
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

#[macro_export]
macro_rules! gen_sql_and4timestamptz {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.eq(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Ne(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.ne(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Gt(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.gt(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Lt(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.lt(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Ge(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.ge(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::Le(val) => {
        let val = $crate::models::parse_date_string(&val).unwrap();
        Box::new($query.and($column.le(val.clone())))
      }
      nanocl_stubs::generic::GenericClause::IsNull => {
        Box::new($query.and($column.is_null()))
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        Box::new($query.and($column.is_not_null()))
      }
      nanocl_stubs::generic::GenericClause::In(items) => {
        let items: Vec<chrono::DateTime<chrono::Utc>> = items
          .iter()
          .map(|item| $crate::models::parse_date_string(item).unwrap())
          .collect();
        Box::new($query.and($column.eq_any(items)))
      }
      nanocl_stubs::generic::GenericClause::NotIn(items) => {
        let items: Vec<chrono::DateTime<chrono::Utc>> = items
          .iter()
          .map(|item| $crate::models::parse_date_string(item).unwrap())
          .collect();
        Box::new($query.and($column.ne_all(items)))
      }
      _ => {
        panic!("Unsupported clause");
      }
    }
  };
}

/// Generate a where clause for a json column
#[macro_export]
macro_rules! gen_sql_where4json {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      nanocl_stubs::generic::GenericClause::Contains(val) => {
        $query = $query.filter($column.contains(val.clone()));
      }
      nanocl_stubs::generic::GenericClause::HasKey(val) => {
        $query = $query.filter($column.has_key(val.clone()));
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

#[macro_export]
macro_rules! gen_sql_and4uuid {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        Box::new($query.and($column.is_null()))
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        Box::new($query.and($column.is_not_null()))
      }
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        let uuid = uuid::Uuid::parse_str(&val).unwrap_or_default();
        Box::new($query.and($column.eq(uuid)))
      }
      _ => {
        panic!("Unsupported clause");
      }
    }
  };
}

#[macro_export]
macro_rules! gen_sql_where4uuid {
  ($query: expr, $column: expr, $value: expr) => {
    match $value {
      nanocl_stubs::generic::GenericClause::IsNull => {
        $query = $query.filter($column.is_null());
      }
      nanocl_stubs::generic::GenericClause::IsNotNull => {
        $query = $query.filter($column.is_not_null());
      }
      nanocl_stubs::generic::GenericClause::Eq(val) => {
        let uuid = uuid::Uuid::parse_str(&val).unwrap_or_default();
        $query = $query.filter($column.eq(uuid));
      }
      _ => {
        // Ignore unsupported clause
      }
    }
  };
}

#[macro_export]
macro_rules! gen_sql_multiple {
  ($query: expr, $filter: expr) => {
    let limit = $filter.limit.unwrap_or(100);
    let offset = $filter.offset.unwrap_or(0);
    $query = $query.limit(limit as i64).offset(offset as i64);
  };
}

#[macro_export]
macro_rules! gen_sql_query {
  ($query:expr, $filter:expr, $columns:expr) => {{
    let r#where = $filter.r#where.to_owned().unwrap_or_default();
    let conditions = r#where.conditions;
    for (key, value) in conditions {
      if let Some(s_column) = $columns.get(key.as_str()) {
        match s_column.0 {
          ColumnType::Uuid => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Uuid>(s_column.1);
            $crate::gen_sql_where4uuid!($query, column, value);
          }
          ColumnType::Json => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Jsonb>(s_column.1);
            $crate::gen_sql_where4json!($query, column, value);
          }
          ColumnType::Text => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Text>(s_column.1);
            $crate::gen_sql_where4string!($query, column, value);
          }
          ColumnType::Timestamptz => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Timestamptz>(s_column.1);
            $crate::gen_sql_where4timestamptz!($query, column, value);
          }
        }
      }
    }
    let or = r#where.or.unwrap_or_default();
    for or in or {
      // dummy condition to start with and then add the generated conditions
      // It's kinda hacky but i didn't find a better way to do it
      let mut or_condition: Box<
        dyn BoxableExpression<_, _, SqlType = diesel::sql_types::Bool>,
      > = Box::new(diesel::dsl::sql::<diesel::sql_types::Bool>("1=1"));
      for (key, value) in or {
        if let Some(s_column) = $columns.get(key.as_str()) {
          match s_column.0 {
            ColumnType::Uuid => {
              let column =
                diesel::dsl::sql::<diesel::sql_types::Uuid>(s_column.1);
              or_condition =
                $crate::gen_sql_and4uuid!(or_condition, column, value);
            }
            ColumnType::Text => {
              let column =
                diesel::dsl::sql::<diesel::sql_types::Text>(s_column.1);
              or_condition =
                $crate::gen_sql_and4string!(or_condition, column, value);
            }
            ColumnType::Json => {
              let column =
                diesel::dsl::sql::<diesel::sql_types::Jsonb>(s_column.1);
              or_condition =
                $crate::gen_sql_and4json!(or_condition, column, value);
            }
            ColumnType::Timestamptz => {
              let column =
                diesel::dsl::sql::<diesel::sql_types::Timestamptz>(s_column.1);
              or_condition =
                $crate::gen_sql_and4timestamptz!(or_condition, column, value);
            }
          }
        }
      }
      $query = $query.or_filter(or_condition);
    }
    $query
  }};
}

#[macro_export]
macro_rules! gen_sql_order_by {
  ($query:expr, $orders:expr, $columns:expr) => {{
    for order in $orders {
      let words: Vec<_> = order.split_whitespace().collect();
      let column = words.first().unwrap_or(&"");
      let order = words.get(1).unwrap_or(&"");
      use std::str::FromStr;
      let order = nanocl_stubs::generic::GenericOrder::from_str(order).unwrap();
      if let Some(s_column) = $columns.get(column) {
        match s_column.0 {
          ColumnType::Uuid => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Uuid>(s_column.1);
            match order {
              nanocl_stubs::generic::GenericOrder::Asc => {
                $query = $query.order(column.asc());
              }
              nanocl_stubs::generic::GenericOrder::Desc => {
                $query = $query.order(column.desc());
              }
            }
          }
          ColumnType::Json => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Json>(s_column.1);
            match order {
              nanocl_stubs::generic::GenericOrder::Asc => {
                $query = $query.order(column.asc());
              }
              nanocl_stubs::generic::GenericOrder::Desc => {
                $query = $query.order(column.desc());
              }
            }
          }
          ColumnType::Text => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Text>(s_column.1);
            match order {
              nanocl_stubs::generic::GenericOrder::Asc => {
                $query = $query.order(column.asc());
              }
              nanocl_stubs::generic::GenericOrder::Desc => {
                $query = $query.order(column.desc());
              }
            }
          }
          ColumnType::Timestamptz => {
            let column =
              diesel::dsl::sql::<diesel::sql_types::Timestamptz>(s_column.1);
            match order {
              nanocl_stubs::generic::GenericOrder::Asc => {
                $query = $query.order(column.asc());
              }
              nanocl_stubs::generic::GenericOrder::Desc => {
                $query = $query.order(column.desc());
              }
            }
          }
        }
      }
    }
    $query
  }};
}
