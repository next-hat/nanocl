---
title: State apply
sidebar_position: 67
---

# State apply

# NAME

apply - Create or Update elements from a Statefile

## SYNOPSIS

**apply** \[**-s**\|**--source**\] \[**-f**\|**--follow**\]
\[**-y**\|**--yes**\] \[**-r**\|**--reload**\] \[**--remove-orphans**\]
\[**--json**\] \[**-h**\|**--help**\] \[*ARGS*\]

## DESCRIPTION

Create or Update elements from a Statefile

Without **-y**, print the complete rendered Statefiles before asking for
confirmation. This includes Statefiles loaded from a URL and their SubStates.
With **-y**, print a concise summary of each rendered source before applying:
namespace, effective group (explicit or derived), object counts, Cargo images
and declared health-check policy. Environment values, secret data, resource
payloads and health-check commands are omitted from this summary. URL
credentials, query strings and fragments are omitted from summary sources.

Progress includes elapsed time and resource names, including when output is
redirected. Long-running steps print a waiting update every 30 seconds in plain
logs. Cargo rollout observations describe phases seen on the event stream;
they do not mark the operation complete. An observed healthy candidate can
still fail later in the rollout. Missing event detail leaves the ordinary
operation progress in place. Optional event subscription setup is limited to
two seconds before apply falls back to ordinary progress.

After each successful Cargo step, a best-effort current snapshot shows its
revision, running application processes, effective health status and image IDs.
Image IDs are not registry manifest digests. Disabled checks are distinguished
from unavailable health information. The snapshot is an observation, not an
extra readiness gate or a guarantee that no concurrent update occurred. A
failed or timed-out snapshot, or a failure to write it, does not fail apply;
use **nanocl state status** to inspect current state. Existing apply completion
and exit-code semantics are unchanged.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*  
Path or Url to the Statefile

**-f**, **--follow**  
Follow logs of the deployed cargo

**-y**, **--yes**  
Skip confirmation and show a concise Statefile summary

**-r**, **--reload**  
Perform an apply even if state didnt changed

**--remove-orphans**  
Remove orphaned elements

**--json**

Emit newline-delimited JSON (NDJSON) for tools and dashboards. Requires
**-y**/**--yes** and cannot be combined with **-f**/**--follow**.
See [State command JSON output](../state-json.md) for the record format.

**-h**, **--help**  
Print help

\[*ARGS*\]  
Additional arguments to pass to the file

## JSON EXAMPLE

```sh
nanocl state apply -s Statefile.yml -y --json
```
