---
title: State remove
sidebar_position: 71
---

# State remove

# NAME

remove - Remove elements from a Statefile

## SYNOPSIS

**remove** \[**-s**\|**--source**\] \[**-y**\|**--yes**\]
\[**--json**\] \[**-h**\|**--help**\] \[*ARGS*\]

## DESCRIPTION

Remove elements from a Statefile

## OPTIONS

**-s**, **--source** *\<SOURCE\>*  
Path or Url to the Statefile

**-y**, **--yes**  
Skip the confirmation prompt

**--json**

Emit newline-delimited JSON (NDJSON) for tools and dashboards. Requires
**-y**/**--yes**. Failed removals produce a failed result and a nonzero exit code.
See [State command JSON output](../state-json.md) for the record format.

**-h**, **--help**  
Print help

\[*ARGS*\]  
Additional arguments to pass to the file

## JSON EXAMPLE

```sh
nanocl state rm -s Statefile.yml -y --json
```
