---
title: Cargo history
sidebar_position: 4
---

# Cargo history

## NAME

history - List Cargo specification revisions or inspect one revision

## SYNOPSIS

**nanocl cargo history** *KEY* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl cargo history** *KEY* **--id** *UUID* [**--display** *FORMAT*]

## DESCRIPTION

List specification revisions for the Cargo identified by its canonical
`namespace.name` key, newest first. Each row identifies a revision that can be
inspected with **--id**. Cargo history describes definitions, while Job runs
describes individual executions.

This command is read-only. Inspecting a revision does not apply it to the live
Cargo. Pagination limits displayed rows; the CLI retrieves the complete revision
history from the existing daemon endpoint before selecting the requested page.

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--id**.

**--offset** *OFFSET*

Nonnegative number of revisions to skip. Defaults to zero. Cannot be used with
**--id**.

**--id** *UUID*

Inspect one specification revision belonging to the selected Cargo.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--id**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format, normally YAML. TOML requires **--id**.

**-h**, **--help**

Print help.

## EXAMPLES

```sh
nanocl cargo history global.api
nanocl cargo history global.api --limit 20 --offset 20
nanocl cargo history global.api --display json
nanocl cargo history global.api --id 12345678-1234-1234-1234-123456789abc
nanocl cargo history global.api --id 12345678-1234-1234-1234-123456789abc --display toml
```
