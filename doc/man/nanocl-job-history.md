---
title: Job history
---

# Job history

## NAME

history - List retained Job executions or inspect one run

## SYNOPSIS

**nanocl job history** *NAME* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl job history** *NAME* **--run** *RUN-ID* [**--display** *FORMAT*]

## DESCRIPTION

List executions from newest to oldest acceptance. Each result includes a stable
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

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--run**.

**--offset** *OFFSET*

Nonnegative number of executions to skip. Defaults to zero. Cannot be used with
**--run**.

**--run** *RUN-ID*

Inspect one retained execution belonging to the named Job.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--run**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format.

## EXAMPLES

```sh
nanocl job history backup
nanocl job history backup --limit 20 --offset 20
nanocl job history backup --display json
nanocl job history backup --run 12345678-1234-1234-1234-123456789abc
```
