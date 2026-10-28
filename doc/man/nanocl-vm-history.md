---
title: VM history
---

# VM history

## NAME

history - List VM specification revisions or inspect one revision

## SYNOPSIS

**nanocl vm history** *KEY* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl vm history** *KEY* **--id** *UUID* [**--display** *FORMAT*]

## DESCRIPTION

List specification revisions for the current incarnation of the VM identified
by its canonical `namespace.name` key, newest first. The VM must still exist.
Each row identifies a revision that can be
inspected with **--id**. VM history describes definitions, while Job history
describes individual executions.

This command is read-only. Inspecting a revision does not apply it to the live
VM. Pagination limits displayed rows; the CLI retrieves the complete revision
history from the existing daemon endpoint before selecting the requested page.

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--id**.

**--offset** *OFFSET*

Nonnegative number of revisions to skip. Defaults to zero. Cannot be used with
**--id**.

**--id** *UUID*

Inspect one specification revision belonging to the selected VM.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--id**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format, normally YAML. TOML requires **--id**.

**-h**, **--help**

Print help.

## EXAMPLES

```sh
nanocl vm history global.worker
nanocl vm history global.worker --limit 20 --offset 20
nanocl vm history global.worker --display yaml
nanocl vm history global.worker --id 12345678-1234-1234-1234-123456789abc
nanocl vm history global.worker --id 12345678-1234-1234-1234-123456789abc --display toml
```
