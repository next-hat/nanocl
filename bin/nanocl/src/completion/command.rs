use std::ffi::{OsStr, OsString};

use clap::{Arg, Command, CommandFactory, ValueHint};
use clap_complete::engine::{
  ArgValueCandidates, ArgValueCompleter, CompletionCandidate,
};

use crate::models::{Cli, CompletionKind};

/// Extend the normal parser only when the shell requests completion.
pub(super) fn command_for(args: &[OsString]) -> Command {
  decorate(Cli::command(), &[], args)
}

fn decorate(command: Command, path: &[&str], args: &[OsString]) -> Command {
  command
    .mut_args(|arg| decorate_arg(arg, path, args))
    .mut_subcommands(|child| {
      let name = child.get_name().to_owned();
      let mut path = path.to_vec();
      path.push(&name);
      decorate(child, &path, args)
    })
}

fn decorate_arg(mut arg: Arg, path: &[&str], args: &[OsString]) -> Arg {
  if !arg.get_action().takes_values() {
    return arg;
  }
  let id = arg.get_id().as_str().to_owned();
  // Unknown hints otherwise fall back to local files, including for secrets,
  // new object names, container image references and remote exec commands.
  arg = arg.value_hint(ValueHint::Other);
  if let Some(kind) = object_kind(path, &id) {
    let args = args.to_vec();
    return arg.add(ArgValueCompleter::new(move |current: &OsStr| {
      super::complete(kind, &args, current)
    }));
  }
  let choices: &'static [&'static str] = match (path, id.as_str()) {
    (["ps"], "kind") => &["cargo", "job", "vm"],
    (["job", "wait"], "condition") => &["next-exit", "not-running", "removed"],
    (["kill"], "signal") => &[
      "SIGHUP",
      "SIGINT",
      "SIGQUIT",
      "SIGILL",
      "SIGTRAP",
      "SIGABRT",
      "SIGBUS",
      "SIGFPE",
      "SIGKILL",
      "SIGUSR1",
      "SIGSEGV",
      "SIGUSR2",
      "SIGPIPE",
      "SIGALRM",
      "SIGTERM",
      "SIGCHLD",
      "SIGCONT",
      "SIGSTOP",
      "SIGTSTP",
      "SIGTTIN",
      "SIGTTOU",
      "SIGURG",
      "SIGXCPU",
      "SIGXFSZ",
      "SIGVTALRM",
      "SIGPROF",
      "SIGWINCH",
      "SIGIO",
      "SIGPWR",
      "SIGSYS",
    ],
    (_, "tail") => &["all"],
    _ => &[],
  };
  if !choices.is_empty() {
    return arg.add(ArgValueCandidates::new(move || {
      choices
        .iter()
        .map(|value| CompletionCandidate::new(*value))
        .collect()
    }));
  }
  let hint = match (path, id.as_str()) {
    (["state", _], "source" | "output")
    | (["exec"], "env_file")
    | (["context", "from"], "path")
    | (["vm", "create" | "run"], "image")
    | (["install" | "uninstall"], "template")
    | (
      ["secret", "create" | "patch", "tls"],
      "certificate_path" | "certificate_key_path" | "certificate_client_path",
    ) => ValueHint::FilePath,
    (["backup"], "output_dir") | (["install"], "state_dir" | "conf_dir") => {
      ValueHint::DirPath
    }
    _ => ValueHint::Other,
  };
  arg.value_hint(hint)
}

fn object_kind(path: &[&str], id: &str) -> Option<CompletionKind> {
  use CompletionKind::*;

  match (path, id) {
    (_, "namespace") => Some(Namespace),
    (
      [
        "cargo",
        "start" | "stop" | "restart" | "remove" | "inspect" | "patch"
        | "history" | "revert" | "logs" | "stats",
      ],
      "key" | "keys",
    ) => Some(Cargo),
    (["cargo", "patch"], "container") => Some(CargoContainer),
    (["cargo", "revert"], "history_id") => Some(CargoHistory),
    (
      [
        "vm",
        "remove" | "inspect" | "start" | "stop" | "attach" | "patch",
      ],
      "key" | "keys",
    ) => Some(Vm),
    (
      ["job", "remove" | "inspect" | "logs" | "wait" | "start"],
      "key" | "keys" | "name",
    ) => Some(Job),
    (["resource", "remove" | "inspect"], "key" | "keys")
    | (["resource", "history" | "revert"], "name") => Some(Resource),
    (["resource", "revert"], "key") => Some(ResourceHistory),
    (["namespace", "remove" | "inspect"], "key" | "keys") => Some(Namespace),
    (["secret", "remove" | "inspect"], "key" | "keys")
    | (["secret", "patch"], "name") => Some(Secret),
    (["context", "use"], "name") => Some(Context),
    (["exec" | "kill"], "process")
    | (["logs" | "stats"], "names")
    | (["inspect"], "key") => Some(Process),
    (["event", "inspect"], "key") => Some(Event),
    (["metric", "inspect"], "key") => Some(Metric),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn argument<'a>(command: &'a Command, path: &[&str], id: &str) -> &'a Arg {
    let command = path.iter().fold(command, |command, name| {
      command.find_subcommand(name).unwrap()
    });
    command
      .get_arguments()
      .find(|arg| arg.get_id() == id)
      .unwrap()
  }

  fn complete(words: &[&str]) -> Vec<String> {
    let args: Vec<_> = words.iter().map(OsString::from).collect();
    let mut command = command_for(&args[..args.len() - 1]);
    clap_complete::engine::complete(&mut command, args, words.len() - 1, None)
      .unwrap()
      .into_iter()
      .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
      .collect()
  }

  #[test]
  fn completes_commands_flags_aliases_and_static_values_without_daemon() {
    assert!(complete(&["nanocl", "car"]).contains(&"cargo".to_owned()));
    assert!(
      complete(&["nanocl", "cargo", "in"]).contains(&"inspect".to_owned())
    );
    assert!(
      complete(&["nanocl", "vm", "ls", "--na"])
        .contains(&"--namespace".to_owned())
    );
    assert_eq!(
      complete(&["nanocl", "cargo", "inspect", "--display", "j"]),
      ["json"]
    );
    assert_eq!(complete(&["nanocl", "ps", "--kind", "v"]), ["vm"]);
    assert_eq!(
      complete(&["nanocl", "job", "wait", "-c", "next"]),
      ["next-exit"]
    );
    assert_eq!(
      complete(&["nanocl", "kill", "--signal", "SIGTE"]),
      ["SIGTERM"]
    );
    assert_eq!(complete(&["nanocl", "logs", "-t", "a"]), ["all"]);
  }

  #[test]
  fn wires_existing_targets_and_dependent_values_without_completing_new_names()
  {
    let command = command_for(&[]);
    for (path, id) in [
      (&["cargo", "inspect"][..], "key"),
      (&["cargo", "remove"], "keys"),
      (&["cargo", "patch"], "container"),
      (&["cargo", "revert"], "history_id"),
      (&["vm", "attach"], "key"),
      (&["job", "wait"], "name"),
      (&["resource", "revert"], "name"),
      (&["resource", "revert"], "key"),
      (&["namespace", "inspect"], "key"),
      (&["secret", "patch"], "name"),
      (&["context", "use"], "name"),
      (&["exec"], "process"),
      (&["kill"], "process"),
      (&["inspect"], "key"),
      (&["logs"], "names"),
      (&["stats"], "names"),
      (&["event", "inspect"], "key"),
      (&["metric", "inspect"], "key"),
      (&["cargo", "create"], "namespace"),
      (&["vm", "run"], "namespace"),
      (&["ps"], "namespace"),
    ] {
      assert!(
        argument(&command, path, id)
          .get::<ArgValueCompleter>()
          .is_some(),
        "missing completion for {path:?} {id}"
      );
    }
    for path in [
      &["cargo", "create"][..],
      &["cargo", "run"],
      &["vm", "create"],
      &["vm", "run"],
      &["namespace", "create"],
      &["secret", "create"],
    ] {
      assert!(
        argument(&command, path, "name")
          .get::<ArgValueCompleter>()
          .is_none(),
        "new names must not suggest existing objects: {path:?}"
      );
    }
    assert_eq!(
      object_kind(&["resource", "revert"], "key"),
      Some(CompletionKind::ResourceHistory)
    );
  }

  #[test]
  fn completes_local_paths_without_suggesting_files_for_remote_or_secret_values()
   {
    let command = command_for(&[]);
    for (path, id, hint) in [
      (&["state", "apply"][..], "source", ValueHint::FilePath),
      (&["state", "status"], "source", ValueHint::FilePath),
      (&["state", "render"], "output", ValueHint::FilePath),
      (&["exec"], "env_file", ValueHint::FilePath),
      (&["context", "from"], "path", ValueHint::FilePath),
      (&["vm", "create"], "image", ValueHint::FilePath),
      (&["backup"], "output_dir", ValueHint::DirPath),
      (&["install"], "state_dir", ValueHint::DirPath),
      (
        &["secret", "patch", "tls"],
        "certificate_key_path",
        ValueHint::FilePath,
      ),
      (
        &["secret", "patch", "tls"],
        "certificate_key",
        ValueHint::Other,
      ),
      (&["vm", "patch"], "password", ValueHint::Other),
      (&["exec"], "command", ValueHint::Other),
      (&["exec"], "workdir", ValueHint::Other),
      (&["cargo", "run"], "image", ValueHint::Other),
    ] {
      assert_eq!(argument(&command, path, id).get_value_hint(), hint);
    }
    assert!(complete(&["nanocl", "vm", "patch", "--password", ""]).is_empty());
    assert!(
      complete(&["nanocl", "secret", "create", "new", "env", "TOKEN="])
        .is_empty()
    );
  }
}
