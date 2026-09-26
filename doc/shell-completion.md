# Shell completion

Nanocl uses [`clap_complete`](https://docs.rs/clap_complete/4.6.11/clap_complete/)
for commands, subcommands, options, enum values, local file paths, and live
object names. Enable it once in your shell configuration, then open a new shell
(or run the same line in the current one).

## Bash

Add to `~/.bashrc`:

```bash
source <(COMPLETE=bash nanocl)
```

## Zsh

Add to `~/.zshrc`, after your existing completion initialization. If you do not
already initialize completion, include the first line too:

```zsh
autoload -Uz compinit && compinit
source <(COMPLETE=zsh nanocl)
```

## Fish

Add to `~/.config/fish/completions/nanocl.fish`:

```fish
COMPLETE=fish nanocl | source
```

## PowerShell

Add to your `$PROFILE`:

```powershell
$env:COMPLETE = 'powershell'
nanocl | Out-String | Invoke-Expression
Remove-Item Env:\COMPLETE
```

## Elvish

Add to `~/.elvish/rc.elv`:

```elvish
eval (E:COMPLETE=elvish nanocl | slurp)
```

The registration code is generated on shell startup so it stays in sync with
Nanocl upgrades. Do not export `COMPLETE` globally: it is only set on completion
invocations. `nanocl` must be on your shell's `PATH`.

## What is completed

Press Tab after a command or partially entered value, for example:

```text
nanocl car<Tab>
nanocl cargo inspect global.<Tab>
nanocl cargo patch global.api --container <Tab>
nanocl job logs ba<Tab>
nanocl resource inspect rou<Tab>
nanocl vm start system.<Tab>
nanocl cargo list --namespace sy<Tab>
nanocl context use prod<Tab>
nanocl exec global.<Tab>
nanocl state apply --source ./<Tab>
```

Existing cargoes and VMs are completed as canonical `{namespace}.{name}` keys
across namespaces. Jobs, resources, secrets, and namespaces use their existing
names. Process commands (`exec`, `kill`, `logs`, `inspect`, and `stats`) suggest
concrete process names or full IDs. Context names come from local configuration.
Cargo container names and recent cargo/resource history IDs are also suggested.
Event and metric inspection accepts ID suggestions. History suggestions cover the
latest 100 revisions exposed by the existing history endpoints; event and metric
ID searches page through the existing list endpoints within the same deadline.

New object names, passwords, secret contents, arbitrary environment values, and
commands inside a container remain free-form input.

Remote suggestions use the same current context, TLS configuration, `-H` /
`--host`, and environment settings as normal Nanocl commands. They only read
existing configuration and daemon data; pressing Tab never creates a context,
switches contexts, or executes the command being completed.

A lookup has a one-second deadline and returns up to 100 sorted, unique matches.
Type a longer prefix to narrow the results. An unavailable daemon, inaccessible
context, failed request, or expired deadline produces no remote suggestions.
Command, option, enum, and local path completion still work without a daemon.
