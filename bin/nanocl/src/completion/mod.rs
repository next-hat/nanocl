//! Shell completion runs before normal command parsing and never dispatches a command.

use std::ffi::{OsStr, OsString};

use clap::{ArgMatches, CommandFactory};
use clap_complete::{CompleteEnv, CompletionCandidate};
use nanocld_client::NanocldClient;

use crate::models::{Cli, CompletionKind, Context};

mod command;
mod query;

const MAX_CANDIDATES: usize = 100;

pub fn complete_env() {
  let Some(shell) = std::env::var_os("COMPLETE") else {
    return;
  };
  if shell.is_empty() || shell == "0" {
    return;
  }
  // Match normal command connection settings, including a local .env file.
  dotenvy::dotenv().ok();
  let args: Vec<_> = std::env::args_os().collect();
  let cursor = std::env::var("_CLAP_COMPLETE_INDEX")
    .ok()
    .and_then(|index| index.parse().ok());
  let args = preceding_args(&args, cursor);
  CompleteEnv::with_factory(|| command::command_for(&args)).complete();
}

/// Shells put the command line after `--`. Only words before the cursor may
/// select the connection or the object for dependent completions.
fn preceding_args(args: &[OsString], cursor: Option<usize>) -> Vec<OsString> {
  let Some(start) = args.iter().position(|arg| arg == "--") else {
    return Vec::new();
  };
  let words = &args[start + 1..];
  let cursor = cursor.unwrap_or_else(|| words.len().saturating_sub(1));
  words[..cursor.min(words.len())].to_vec()
}

fn partial_matches(args: &[OsString]) -> Option<ArgMatches> {
  if args.is_empty() {
    return None;
  }
  Cli::command()
    .ignore_errors(true)
    .try_get_matches_from(args)
    .ok()
}

fn selected_object(matches: &ArgMatches, kind: CompletionKind) -> Option<&str> {
  let (group, group_matches) = matches.subcommand()?;
  let (operation, opts) = group_matches.subcommand()?;
  let field = match (kind, group, operation) {
    (CompletionKind::CargoContainer, "cargo", "patch")
    | (CompletionKind::CargoHistory, "cargo", "revert") => "key",
    (CompletionKind::ResourceHistory, "resource", "revert") => "name",
    _ => return None,
  };
  opts
    .try_get_one::<String>(field)
    .ok()
    .flatten()
    .map(String::as_str)
}

fn complete(
  kind: CompletionKind,
  args: &[OsString],
  current: &OsStr,
) -> Vec<CompletionCandidate> {
  let Some(current) = current.to_str() else {
    return Vec::new();
  };
  let values = if kind == CompletionKind::Context {
    context_names()
  } else {
    let Some(matches) = partial_matches(args) else {
      return Vec::new();
    };
    let host = matches.get_one::<String>("host").map(String::as_str);
    let Ok(config) = crate::load_cli_config(host, true) else {
      return Vec::new();
    };
    // The HTTP client expects complete TLS credentials and parses the CA with
    // `expect`. Invalid completion configuration must not panic on each Tab.
    if let Some(ssl) = &config.client.ssl {
      if ssl.cert.is_none() || ssl.cert_key.is_none() {
        return Vec::new();
      }
      if ssl.verify
        && ssl.cert_ca.as_ref().is_none_or(|ca| {
          openssl::x509::X509::from_pem(ca.as_bytes()).is_err()
        })
      {
        return Vec::new();
      }
    }
    remote_candidates(
      config.client,
      kind,
      current.to_owned(),
      selected_object(&matches, kind).map(str::to_owned),
    )
  };
  matching_candidates(values, current)
}

#[ntex::main]
async fn remote_candidates(
  client: NanocldClient,
  kind: CompletionKind,
  current: String,
  selected: Option<String>,
) -> Vec<String> {
  // The deadline covers the entire lookup, including dependent requests.
  ntex::time::timeout(
    ntex::time::Millis(1000),
    query::candidates(&client, kind, &current, selected.as_deref()),
  )
  .await
  .ok()
  .and_then(Result::ok)
  .unwrap_or_default()
}

fn context_names() -> Vec<String> {
  let mut names = vec!["default".to_owned()];
  let Some(home) = std::env::var_os("HOME") else {
    return names;
  };
  let path = std::path::PathBuf::from(home).join(".nanocl/contexts");
  let Ok(entries) = std::fs::read_dir(path) else {
    return names;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if let Some(path) = path.to_str()
      && let Ok(context) = Context::read(path)
    {
      names.push(context.name);
    }
  }
  names
}

fn matching_candidates(
  values: Vec<String>,
  current: &str,
) -> Vec<CompletionCandidate> {
  let mut values: Vec<_> = values
    .into_iter()
    .filter(|value| {
      value.starts_with(current) && !value.chars().any(char::is_control)
    })
    .collect();
  values.sort();
  values.dedup();
  values.truncate(MAX_CANDIDATES);
  values.into_iter().map(CompletionCandidate::new).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn words(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
  }

  #[test]
  fn completion_cursor_excludes_current_word_and_later_arguments() {
    let args = words(&[
      "/bin/nanocl",
      "--",
      "nanocl",
      "cargo",
      "inspect",
      "gl",
      "--host",
      "wrong",
    ]);
    assert_eq!(
      preceding_args(&args, Some(3)),
      words(&["nanocl", "cargo", "inspect"])
    );
    assert!(preceding_args(&words(&["nanocl"]), None).is_empty());
    assert_eq!(
      preceding_args(
        &words(&["nanocl", "--", "nanocl", "context", "use", ""]),
        None
      ),
      words(&["nanocl", "context", "use"]),
    );
    // Zsh can omit the empty word at the cursor.
    assert_eq!(
      preceding_args(&args[..5], Some(3)),
      words(&["nanocl", "cargo", "inspect"])
    );
  }

  #[test]
  fn completion_partial_parse_preserves_host_and_dependent_object() {
    for host in [
      words(&["-H", "http://example:8585"]),
      words(&["--host=http://example:8585"]),
      words(&["-Hhttp://example:8585"]),
    ] {
      let mut args = words(&["nanocl"]);
      args.extend(host);
      args.extend(words(&["cargo", "patch", "global.api", "--container"]));
      let matches = partial_matches(&args).unwrap();
      assert_eq!(
        matches.get_one::<String>("host").unwrap(),
        "http://example:8585"
      );
      assert_eq!(
        selected_object(&matches, CompletionKind::CargoContainer),
        Some("global.api")
      );
    }
    let matches =
      partial_matches(&words(&["nanocl", "resource", "revert", "route"]))
        .unwrap();
    assert_eq!(
      selected_object(&matches, CompletionKind::ResourceHistory),
      Some("route")
    );
    let matches =
      partial_matches(&words(&["nanocl", "cargo", "revert", "system.api"]))
        .unwrap();
    assert_eq!(
      selected_object(&matches, CompletionKind::CargoHistory),
      Some("system.api")
    );
    let matches = partial_matches(&words(&[
      "nanocl",
      "exec",
      "process",
      "sh",
      "--host=wrong",
    ]))
    .unwrap();
    assert!(matches.get_one::<String>("host").is_none());
  }

  #[test]
  fn completion_candidates_are_literal_sorted_unique_and_bounded() {
    let values = [
      "global.z",
      "global.a",
      "global.a",
      "global.\nbad",
      "other.a",
      "global.\tbad",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let candidates = matching_candidates(values, "global.");
    let values: Vec<_> =
      candidates.iter().map(|value| value.get_value()).collect();
    assert_eq!(values, [OsStr::new("global.a"), OsStr::new("global.z")]);
    let candidates = matching_candidates(
      (0..150).map(|i| format!("job{i:03}")).collect(),
      "job",
    );
    assert_eq!(candidates.len(), MAX_CANDIDATES);
    assert_eq!(candidates.last().unwrap().get_value(), "job099");
    assert!(matching_candidates(vec!["job".to_owned()], "j%").is_empty());
  }
}
