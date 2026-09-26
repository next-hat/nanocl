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

## OPTIONS

**-s**, **--source** *\<SOURCE\>*  
Path or Url to the Statefile

**-f**, **--follow**  
Follow logs of the deployed cargo

**-y**, **--yes**  
Skip the confirmation prompt

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
