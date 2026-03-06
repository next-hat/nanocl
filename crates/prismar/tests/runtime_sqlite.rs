use diesel::{
  connection::SimpleConnection,
  RunQueryDsl,
  dsl::sql,
  r2d2::{ConnectionManager, Pool},
  sql_types::Integer,
  sqlite::SqliteConnection,
};
use prismar::with_connection;

#[ntex::test]
async fn executes_diesel_query_on_ntex_runtime() {
  let manager = ConnectionManager::<SqliteConnection>::new(":memory:");
  let pool = Pool::builder().max_size(1).build(manager).unwrap();

  with_connection(pool.clone(), |conn| {
    conn.batch_execute("CREATE TABLE values_table (id INTEGER PRIMARY KEY, val INTEGER NOT NULL);")?;
    conn.batch_execute("INSERT INTO values_table (val) VALUES (41);")?;
    Ok(())
  })
  .await
  .unwrap();

  let result = with_connection(pool, |conn| {
    diesel::select(sql::<Integer>("SUM(val) + 1 FROM values_table")).get_result::<i32>(conn)
  })
  .await
  .unwrap();

  assert_eq!(result, 42);
}
