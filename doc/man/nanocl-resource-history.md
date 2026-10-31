---
title: Resource history
sidebar_position: 48
---

# Resource history

## NAME

history - List Resource specification revisions or inspect one revision

## SYNOPSIS

**nanocl resource history** *KEY* [**--limit** *LIMIT*] [**--offset** *OFFSET*] [**--display** *FORMAT*]

**nanocl resource history** *KEY* **--id** *UUID* [**--display** *FORMAT*]

## DESCRIPTION

List specification revisions for the resource identified by its global name,
newest first. Each row identifies a revision that can be inspected with **--id**.
Resource history describes definitions, while Job runs describe individual
executions.

This command is read-only. Inspecting a revision does not apply it to the live
resource. Pagination limits displayed rows; the CLI retrieves the complete
revision history from the existing daemon endpoint before selecting the requested
page.

## OPTIONS

**--limit** *LIMIT*

Page size between 1 and 100. Defaults to 50. Cannot be used with **--id**.

**--offset** *OFFSET*

Nonnegative number of revisions to skip. Defaults to zero. Cannot be used with
**--id**.

**--id** *UUID*

Inspect one specification revision belonging to the selected Resource.

**--display** *FORMAT*

Lists support `yaml` or `json` and default to a table. Detail with **--id**
supports `yaml`, `json`, or `toml` and defaults to the configured inspection
format, normally YAML. TOML requires **--id**.

**-h**, **--help**

Print help.

## EXAMPLES

```sh
nanocl resource history web-route
nanocl resource history web-route --limit 20 --offset 20
nanocl resource history web-route --display json
nanocl resource history web-route --id 12345678-1234-1234-1234-123456789abc
nanocl resource history web-route --id 12345678-1234-1234-1234-123456789abc --display toml
```
