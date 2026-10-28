---
title: Job runs
---

# Job runs

## NAME

runs - List retained Job executions or inspect one run

## SYNOPSIS

**nanocl job runs** *KEY* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl job runs** *KEY* **--id** *UUID* [**--display** *FORMAT*]

## DESCRIPTION

The positional *KEY* is the global Job name. List executions from newest to
oldest acceptance. Each result includes a stable
run ID and incarnation ID, so executions of recreated Jobs with the same name
remain distinguishable. History remains available after Job removal, TTL cleanup,
or removal of its Docker containers.

Inspecting a run returns its exact accepted specification, initiating operation,
timestamps, outcome or reason, and ordered step results. The definition contains
authored secret references, without resolved secret injection. A missing exit
code means it was not observed. An `Unknown` run still blocks a conflicting
execution until runtime evidence or normal Job stop/delete resolves it.

A completed start operation acknowledges execution dispatch; the associated run
records the workload's eventual result. History retention does not archive Docker
logs. Use `nanocl job logs` while the relevant runtime logs remain available.

Use `nanocl job history` to inspect configuration revisions. Execution history
previously exposed by that command is now available through `nanocl job runs`.

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--id**.
Pagination is applied by the server.

**--offset** *OFFSET*

Nonnegative number of executions to skip. Defaults to zero. Cannot be used with
**--id**.

**--id** *UUID*

Inspect one retained execution by run UUID belonging to the named Job.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--id**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format, normally YAML. TOML requires **--id**.

## EXAMPLES

```sh
nanocl job runs backup
nanocl job runs backup --limit 20 --offset 20
nanocl job runs backup --display json
nanocl job runs backup --id 12345678-1234-1234-1234-123456789abc
nanocl job runs backup --id 12345678-1234-1234-1234-123456789abc --display toml
```
