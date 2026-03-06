# prismar

`prismar` provides Prisma-like, type-safe filter builders for Rust structs and renders backend-aware SQL fragments for Diesel (`Postgres`, `MySQL`, `SQLite`).

## Highlights

- Derive-generated typed filters: `#[derive(PrismarModel)]`
- Logical composition: `and`, `or`, `not`
- Advanced JSON operators (`contains`, key checks, path predicates, `like` / `notLike`)
- Relation filters (`some`, `every`, `none`, `is`, `isNot`)
- ntex + Diesel helper to execute blocking DB work safely
- Request payload models for read/create/update/delete/count-style operations

## Features

- `derive` (default): enables `PrismarModel` derive macro
- `nanocl_compat`: conversion to `nanocl_stubs::generic::GenericFilter`
- `utoipa`: adds `utoipa::ToSchema` derives to filter and operation payload types

## CRUD operation payloads

The crate exposes API-friendly payload models:

- `ReadManyArgs`, `ReadUniqueArgs`, `CountArgs`
- `CreateArgs`, `CreateManyArgs`
- `UpdateArgs`, `UpdateManyArgs`
- `DeleteArgs`, `DeleteManyArgs`
- `ReadByIdArgs`, `DeleteByIdArgs`

## Fluent typed filter API

With `#[derive(PrismarModel)]`, generated model filters avoid quoted field names:

```rust
let filter = CargoDbFilter::new()
	.namespace_name(|f| f.eq("prod"))
	.name(|f| f.starts_with("api").not_contains("deprecated"))
	.or_where(|q| q.name(|f| f.eq("worker")))
	.build();
```

## Security

`prismar` always binds user values as SQL parameters. Field identifiers are
validated with a strict allow-list pattern (`[A-Za-z_][A-Za-z0-9_.]*`) before
rendering SQL; invalid identifiers fail closed.

Use `RawSqlBuilder` for custom metrics queries while keeping identifier
validation and parameterized values:

```rust
let query = RawSqlBuilder::new(SqlBackend::Postgres, "SELECT * FROM metrics")?
	.filter(ModelFilter::read_by_id("namespace_name", "prod"))
	.order_by("namespace_name", OrderDirection::Asc)?
	.limit(100)
	.build()?;
```

## Coverage and tests

Run tests for this crate:

```bash
cargo test -p prismar --all-features
```

Run coverage (if `cargo-llvm-cov` is installed):

```bash
cargo llvm-cov -p prismar --all-features --lcov --output-path target/prismar.lcov
```
