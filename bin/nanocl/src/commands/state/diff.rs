//! Read-only preview of the existing Statefile apply workflow.

use std::collections::{BTreeMap, BTreeSet};

use nanocl_error::{
  http_client::{HttpClientError, HttpClientResult},
  io::{IoError, IoResult},
};
use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericClause, GenericFilter, GenericFilterNsp},
    job::JobPartial,
    resource::ResourcePartial,
    secret::SecretPartial,
    statefile::Statefile,
    vm_spec::VmSpecPartial,
  },
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
  config::CliConfig,
  models::{
    StateDiff, StateDiffItem, StateDiffOpts, StateDiffSnapshot, StateRef,
  },
  utils::{
    cargo::cargo_spec_from_revision, process::resource_key,
    state_diff::diff_item, state_diff_pager,
  },
};

use super::{
  ArgParseMode, gen_client, get_nanocl_group, parse_build_args,
  parse_state_file_recurr, read_state_file,
};

// Parsing, template, and daemon errors can contain secret input or response data.
fn input_error(stage: &str, err: IoError) -> IoError {
  IoError::with_context(
    format!("State diff: {stage}"),
    std::io::Error::new(
      err.inner.kind(),
      "failed; details hidden to protect secret values",
    ),
  )
}

fn read_error(err: HttpClientError) -> IoError {
  match err {
    HttpClientError::HttpError(err) => IoError::other(
      "State diff",
      &format!("daemon read failed (HTTP {})", err.status.as_u16()),
    ),
    HttpClientError::IoError(err) => input_error("daemon read", err),
  }
}

/// A failed connection or denied read must never become a planned create.
fn optional_read<T>(result: HttpClientResult<T>) -> IoResult<Option<T>> {
  match result {
    Ok(value) => Ok(Some(value)),
    Err(HttpClientError::HttpError(err)) if err.status.as_u16() == 404 => {
      Ok(None)
    }
    Err(err) => Err(read_error(err)),
  }
}

async fn inspect_object(
  client: &NanocldClient,
  kind: &str,
  name: &str,
) -> IoResult<Option<Value>> {
  let value = match kind {
    "secret" => optional_read(client.inspect_secret(name).await)?
      .map(|value| serde_json::to_value(SecretPartial::from(value))),
    "cargo" => optional_read(client.inspect_cargo(name).await)?
      .map(|value| serde_json::to_value(cargo_spec_from_revision(&value.spec))),
    "vm" => optional_read(client.inspect_vm(name).await)?
      .map(|value| serde_json::to_value(VmSpecPartial::from(value.spec))),
    "job" => optional_read(client.inspect_job(name).await)?
      .map(|value| serde_json::to_value(JobPartial::from(value))),
    "resource" => optional_read(client.inspect_resource(name).await)?
      .map(|value| serde_json::to_value(ResourcePartial::from(value))),
    _ => unreachable!("internal object kind"),
  };
  Ok(value.transpose()?)
}

fn desired_value(value: &impl Serialize, group: &str) -> IoResult<Value> {
  let mut value = serde_json::to_value(value)?;
  let object = value.as_object_mut().expect("declaration is an object");
  let metadata = object.entry("Metadata").or_insert_with(|| json!({}));
  let metadata = metadata.as_object_mut().ok_or_else(|| {
    IoError::invalid_input("State diff", "Metadata must be an object")
  })?;
  metadata.insert("io.nanocl.group".into(), json!(group));
  Ok(value)
}

fn desired_objects(
  state: &StateRef<Statefile>,
) -> IoResult<Vec<(String, String, Value)>> {
  let mut objects = Vec::new();
  let namespace = state.data.namespace.as_deref().unwrap_or("global");
  let group = get_nanocl_group(state);
  for value in state.data.secrets.iter().flatten() {
    objects.push((
      "secret".into(),
      value.name.clone(),
      desired_value(value, &group)?,
    ));
  }
  for value in state.data.jobs.iter().flatten() {
    value.validate_schedule().map_err(|_| {
      IoError::invalid_input("State diff", "invalid job schedule")
    })?;
    objects.push((
      "job".into(),
      value.name.clone(),
      desired_value(value, &group)?,
    ));
  }
  for value in state.data.cargoes.iter().flatten() {
    objects.push((
      "cargo".into(),
      resource_key(&value.name, namespace)?,
      desired_value(value, &group)?,
    ));
  }
  for value in state.data.virtual_machines.iter().flatten() {
    objects.push((
      "vm".into(),
      resource_key(&value.name, namespace)?,
      desired_value(value, &group)?,
    ));
  }
  for value in state.data.resources.iter().flatten() {
    objects.push((
      "resource".into(),
      value.name.clone(),
      desired_value(value, &group)?,
    ));
  }
  Ok(objects)
}

/// Diff must display every orphan, even though current apply cleanup has a
/// one-page limit. This helper performs reads only and is not used by apply.
async fn read_all<T, F, Fut>(
  filter: &GenericFilter,
  mut fetch: F,
) -> HttpClientResult<Vec<T>>
where
  F: FnMut(GenericFilter) -> Fut,
  Fut: std::future::Future<Output = HttpClientResult<Vec<T>>>,
{
  let mut filter = filter.clone();
  filter.limit = Some(100);
  filter.order_by = Some(vec!["key asc".into()]);
  let mut items = Vec::new();
  loop {
    filter.offset = Some(items.len());
    let page = fetch(filter.clone()).await?;
    let done = page.len() < 100;
    items.extend(page);
    if done {
      return Ok(items);
    }
  }
}

/// Load ownership-scoped objects, using declarations rather than runtime data.
async fn group_objects(
  client: &NanocldClient,
  state: &StateRef<Statefile>,
) -> IoResult<BTreeMap<(String, String), Value>> {
  let namespace = state.data.namespace.as_deref().unwrap_or("global");
  let filter = GenericFilter::new().r#where(
    "metadata",
    GenericClause::Contains(json!({
      "io.nanocl.group": get_nanocl_group(state),
    })),
  );
  let mut objects = BTreeMap::new();
  for value in read_all(&filter, |page| async move {
    client.list_secret(Some(&page)).await
  })
  .await
  .map_err(read_error)?
  {
    let value = SecretPartial::from(value);
    objects.insert(
      ("secret".into(), value.name.clone()),
      serde_json::to_value(value)?,
    );
  }
  // Collection reads require an existing namespace. Keep global inventories
  // available when a namespace would be created by the apply.
  if optional_read(client.inspect_namespace(namespace).await)?.is_some() {
    for value in read_all(&filter, |page| async move {
      client
        .list_cargo(Some(&GenericFilterNsp {
          filter: Some(page),
          namespace: Some(namespace.into()),
        }))
        .await
    })
    .await
    .map_err(read_error)?
    {
      let value = cargo_spec_from_revision(&value.spec);
      objects.insert(
        ("cargo".into(), resource_key(&value.name, namespace)?),
        serde_json::to_value(value)?,
      );
    }
    for value in read_all(&filter, |page| async move {
      client
        .list_vm(Some(&GenericFilterNsp {
          filter: Some(page),
          namespace: Some(namespace.into()),
        }))
        .await
    })
    .await
    .map_err(read_error)?
    {
      let value = VmSpecPartial::from(value.spec);
      objects.insert(
        ("vm".into(), resource_key(&value.name, namespace)?),
        serde_json::to_value(value)?,
      );
    }
  }
  for value in read_all(&filter, |page| async move {
    client.list_resource(Some(&page)).await
  })
  .await
  .map_err(read_error)?
  {
    let value = ResourcePartial::from(value);
    objects.insert(
      ("resource".into(), value.name.clone()),
      serde_json::to_value(value)?,
    );
  }
  for value in
    read_all(
      &filter,
      |page| async move { client.list_job(Some(&page)).await },
    )
    .await
    .map_err(read_error)?
  {
    let value = JobPartial::from(value.spec);
    objects.insert(
      ("job".into(), value.name.clone()),
      serde_json::to_value(value)?,
    );
  }
  Ok(objects)
}

fn in_scope(
  kind: &str,
  name: &str,
  value: &Value,
  state: &StateRef<Statefile>,
) -> bool {
  if value
    .pointer("/Metadata/io.nanocl.group")
    .and_then(Value::as_str)
    != Some(get_nanocl_group(state).as_str())
  {
    return false;
  }
  if matches!(kind, "cargo" | "vm") {
    let namespace = state.data.namespace.as_deref().unwrap_or("global");
    return value
      .get("Name")
      .and_then(Value::as_str)
      .and_then(|name| resource_key(name, namespace).ok())
      .as_deref()
      == Some(name);
  }
  true
}

fn orphan_removable(kind: &str, state: &Statefile) -> bool {
  match kind {
    "secret" => state.secrets.is_some(),
    "cargo" => state.cargoes.is_some(),
    "vm" => state.virtual_machines.is_some(),
    "resource" => state.resources.is_some(),
    _ => false,
  }
}

fn plan_orphans(
  state: &StateRef<Statefile>,
  opts: &StateDiffOpts,
  desired: &[(String, String, Value)],
  mut objects: BTreeMap<(String, String), Value>,
  snapshot: &mut StateDiffSnapshot,
) -> IoResult<Vec<StateDiffItem>> {
  // Simulate earlier SubStates in apply order, including group changes/removals.
  for ((kind, name), value) in snapshot.iter() {
    objects.remove(&(kind.clone(), name.clone()));
    if let Some(value) = value
      .as_ref()
      .filter(|value| in_scope(kind, name, value, state))
    {
      objects.insert((kind.clone(), name.clone()), value.clone());
    }
  }
  // Do not promise removals that current apply may miss. Its unsorted first
  // page cannot be simulated reliably once an ownership group exceeds 100.
  if opts.remove_orphans {
    for kind in ["secret", "cargo", "vm", "resource"] {
      if orphan_removable(kind, &state.data)
        && objects
          .iter()
          .filter(|((k, n), v)| k == kind && in_scope(k, n, v, state))
          .count()
          > 100
      {
        return Err(IoError::invalid_input(
          "State diff",
          "Cannot reliably preview --remove-orphans for a selected kind with more than 100 objects: apply currently reads only one page. Run diff without --remove-orphans to view all orphans.",
        ));
      }
    }
  }
  let mut items = Vec::new();
  for ((kind, name), value) in objects {
    if !in_scope(&kind, &name, &value, state)
      || desired.iter().any(|(k, n, _)| k == &kind && n == &name)
    {
      continue;
    }
    let removable = orphan_removable(&kind, &state.data);
    let remove = opts.remove_orphans && removable;
    let reason = if remove {
      "orphan removed by apply --remove-orphans"
    } else if kind == "job" {
      "orphan retained: apply does not prune jobs"
    } else if !removable {
      "orphan retained: section omitted from Statefile"
    } else {
      "orphan retained: use --remove-orphans to preview removal"
    };
    items.push(diff_item(
      &state.location,
      &kind,
      &name,
      Some(&value),
      (!remove).then_some(&value),
      false,
      true,
      Some(reason),
    ));
    snapshot.insert((kind, name), (!remove).then_some(value));
  }
  Ok(items)
}

pub(super) async fn exec_state_diff(
  cli_conf: &CliConfig,
  opts: &StateDiffOpts,
) -> IoResult<()> {
  let state =
    read_state_file(&opts.source, &cli_conf.user_config.display_format)
      .await
      .map_err(|err| input_error("read Statefile", err))?;
  // Always use the returning parser: clap errors may echo sensitive arguments.
  let args =
    parse_build_args(&state.data, ArgParseMode::Diff, &opts.args, true)
      .map_err(|err| input_error("Statefile arguments", err))?;
  let states = parse_state_file_recurr(cli_conf, &state, &args, true)
    .await
    .map_err(|err| {
      input_error(
        "render Statefile (only existing Namespaces are available)",
        err,
      )
    })?;
  let mut items = Vec::new();
  let mut namespaces = BTreeSet::new();
  let mut snapshot = StateDiffSnapshot::new();
  for state in &states {
    let namespace = state.data.namespace.as_deref().unwrap_or("global");
    if namespaces.insert((state.data.api_version.clone(), namespace.to_owned()))
    {
      let client = gen_client(cli_conf, state)
        .map_err(|err| input_error("Statefile client", err))?;
      let exists =
        optional_read(client.inspect_namespace(namespace).await)?.is_some();
      let value = json!({"Name": namespace});
      items.push(diff_item(
        &state.location,
        "namespace",
        namespace,
        exists.then_some(&value),
        Some(&value),
        false,
        false,
        None,
      ));
    }
    let desired =
      desired_objects(state).map_err(|err| input_error("declarations", err))?;
    let objects = group_objects(&cli_conf.client, state).await?;
    items.extend(plan_orphans(state, opts, &desired, objects, &mut snapshot)?);
    for (kind, name, after) in desired {
      let key = (kind.clone(), name.clone());
      let before = match snapshot.get(&key) {
        Some(value) => value.clone(),
        None => inspect_object(&cli_conf.client, &kind, &name).await?,
      };
      let reason = if before.is_some() && kind == "job" {
        Some("apply recreates existing jobs")
      } else if before.is_some()
        && opts.reload
        && matches!(kind.as_str(), "cargo" | "vm" | "resource")
      {
        Some("apply --reload forces an update")
      } else {
        None
      };
      items.push(diff_item(
        &state.location,
        &kind,
        &name,
        before.as_ref(),
        Some(&after),
        reason.is_some(),
        false,
        reason,
      ));
      snapshot.insert(key, Some(after));
    }
  }
  // Print only after the entire preview succeeds, so failures never emit a partial plan.
  state_diff_pager::print(&StateDiff::new(items), opts)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::models::{DisplayFormat, StateDiffAction, StateRoot};
  use nanocl_error::http::HttpError;

  fn state(yaml: &str) -> StateRef<Statefile> {
    StateRef {
      raw: String::new(),
      format: DisplayFormat::Yaml,
      root: StateRoot::None,
      location: "state.yml".into(),
      data: serde_yaml::from_str(yaml).unwrap(),
    }
  }

  fn opts(remove_orphans: bool) -> StateDiffOpts {
    StateDiffOpts {
      source: None,
      json: false,
      pager: false,
      no_pager: false,
      reload: false,
      args: vec![],
      remove_orphans,
    }
  }

  #[test]
  fn only_http_not_found_means_absent_and_errors_hide_response_data() {
    assert_eq!(
      optional_read::<u8>(Err(HttpError::not_found("private").into())).unwrap(),
      None
    );
    assert_eq!(optional_read(Ok(42)).unwrap(), Some(42));
    for error in [
      HttpError::forbidden("private"),
      HttpError::internal_server_error("private"),
    ] {
      let error = optional_read::<u8>(Err(error.into())).unwrap_err();
      assert!(!error.to_string().contains("private"));
    }
    assert!(
      optional_read::<u8>(Err(IoError::not_found("socket", "private").into()))
        .is_err()
    );
  }

  #[test]
  fn orphan_preview_preserves_omitted_sections_jobs_and_other_scopes() {
    let state =
      state("ApiVersion: v0.18\nGroup: demo\nNamespace: app\nCargoes: []\n");
    let make = |name: &str, group: &str| json!({"Name": name, "Metadata": {"io.nanocl.group": group}});
    let objects = BTreeMap::from([
      (("cargo".into(), "app.old".into()), make("old", "demo")),
      (("cargo".into(), "other.old".into()), make("old", "demo")),
      (
        ("cargo".into(), "app.unrelated".into()),
        make("unrelated", "other"),
      ),
      (("secret".into(), "secret".into()), make("secret", "demo")),
      (("job".into(), "job".into()), make("job", "demo")),
    ]);
    let retained = plan_orphans(
      &state,
      &opts(false),
      &[],
      objects.clone(),
      &mut StateDiffSnapshot::new(),
    )
    .unwrap();
    assert_eq!(retained.len(), 3);
    assert!(
      retained
        .iter()
        .all(|item| item.orphan && item.action == StateDiffAction::Unchanged)
    );
    let removed = plan_orphans(
      &state,
      &opts(true),
      &[],
      objects,
      &mut StateDiffSnapshot::new(),
    )
    .unwrap();
    assert_eq!(
      removed
        .iter()
        .filter(|item| item.action == StateDiffAction::Remove)
        .count(),
      1
    );
    assert_eq!(
      removed
        .iter()
        .find(|item| item.kind == "cargo")
        .unwrap()
        .name,
      "app.old"
    );
  }

  #[test]
  fn orphan_preview_accounts_for_prior_substates_and_declared_names() {
    let state =
      state("ApiVersion: v0.18\nGroup: demo\nNamespace: app\nCargoes: []\n");
    let value = json!({"Name": "old", "Metadata": {"io.nanocl.group": "demo"}});
    let key = ("cargo".into(), "app.old".into());
    let mut snapshot =
      StateDiffSnapshot::from([(key.clone(), Some(value.clone()))]);
    let declared = vec![("cargo".into(), "app.old".into(), value.clone())];
    assert!(
      plan_orphans(
        &state,
        &opts(true),
        &declared,
        BTreeMap::new(),
        &mut snapshot
      )
      .unwrap()
      .is_empty()
    );
    let removed =
      plan_orphans(&state, &opts(true), &[], BTreeMap::new(), &mut snapshot)
        .unwrap();
    assert_eq!(removed[0].action, StateDiffAction::Remove);
    assert!(
      plan_orphans(
        &state,
        &opts(true),
        &[],
        BTreeMap::from([(key, value)]),
        &mut snapshot
      )
      .unwrap()
      .is_empty()
    );
  }
  #[test]
  fn reads_all_inventory_pages_and_preserves_filter() {
    for count in [0, 100, 235] {
      let filter = GenericFilter::new().r#where(
        "metadata",
        GenericClause::Contains(json!({"io.nanocl.group": "demo"})),
      );
      let mut offsets = Vec::new();
      let result = futures::executor::block_on(read_all(&filter, |page| {
        assert_eq!(page.limit, Some(100));
        assert_eq!(page.order_by, Some(vec!["key asc".into()]));
        assert_eq!(
          serde_json::to_value(&page.r#where).unwrap(),
          serde_json::to_value(&filter.r#where).unwrap()
        );
        let offset = page.offset.unwrap();
        offsets.push(offset);
        std::future::ready(Ok(
          (offset..count.min(offset + 100)).collect::<Vec<_>>(),
        ))
      }))
      .unwrap();
      assert_eq!(result, (0..count).collect::<Vec<_>>());
      assert_eq!(offsets.len(), count / 100 + 1);
    }
    let result =
      futures::executor::block_on(read_all(&GenericFilter::new(), |page| {
        std::future::ready(if page.offset == Some(0) {
          Ok(vec![0; 100])
        } else {
          Err(nanocl_error::http::HttpError::forbidden("denied").into())
        })
      }));
    assert!(result.is_err());
  }

  #[test]
  fn previews_all_orphans_but_refuses_to_guess_large_group_removals() {
    let state = state("ApiVersion: v0.18\nGroup: demo\nCargoes: []\n");
    let objects: BTreeMap<_, _> = (0..101)
      .map(|i| {
        let name = format!("web{i}");
        (
          ("cargo".into(), format!("global.{name}")),
          json!({"Name": name, "Metadata": {"io.nanocl.group": "demo"}}),
        )
      })
      .collect();
    let items = plan_orphans(
      &state,
      &opts(false),
      &[],
      objects.clone(),
      &mut StateDiffSnapshot::new(),
    )
    .unwrap();
    assert_eq!(items.len(), 101);
    assert!(
      items
        .iter()
        .all(|i| i.orphan && i.action == StateDiffAction::Unchanged)
    );
    assert!(
      plan_orphans(
        &state,
        &opts(true),
        &[],
        objects,
        &mut StateDiffSnapshot::new()
      )
      .is_err()
    );
  }
}
