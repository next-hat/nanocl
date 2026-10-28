---
title: Job history
---

# Job history

## NAME

history - List retained Job configuration revisions or inspect one revision

## SYNOPSIS

**nanocl job history** *KEY* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl job history** *KEY* **--id** *UUID* [**--display** *FORMAT*]

## DESCRIPTION

The positional *KEY* is the global Job name. List immutable configuration
revisions from newest to oldest creation, including the current configuration.
Each revision contains its specification UUID, Job key, version, creation time,
and authored declaration. Secret references are retained without resolved secret
injection. A revision is available even if the Job has never run.

Configuration history remains available after Job removal or TTL cleanup and
includes earlier incarnations of a Job recreated with the same name. Statefile
apply deletes and recreates Jobs, retaining the previous configurations.
Inspecting history is read-only and does not restore a revision.

Like Cargo and VM history, this command displays configurations. Use
`nanocl job runs` for execution outcomes, pinned definitions, and ordered step
results. Execution history previously exposed by `nanocl job history` has moved
to `nanocl job runs`; existing run UUIDs belong to the latter command.

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--id**.
Pagination is applied by the server.

**--offset** *OFFSET*

Nonnegative number of revisions to skip. Defaults to zero. Cannot be used with
**--id**.

**--id** *UUID*

Inspect one retained configuration by specification UUID belonging to the named
Job. This is the revision's `Key`, not a run UUID.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--id**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format, normally YAML. TOML requires **--id**.

## EXAMPLES

```sh
nanocl job history backup
nanocl job history backup --limit 20 --offset 20
nanocl job history backup --display json
nanocl job history backup --id 12345678-1234-1234-1234-123456789abc
nanocl job history backup --id 12345678-1234-1234-1234-123456789abc --display toml
nanocl job runs backup
```
