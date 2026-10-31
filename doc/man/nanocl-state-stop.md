---
title: State stop
sidebar_position: 72
---

# State stop

# NAME

stop - Stop cargoes, VMs, and jobs from a Statefile

## SYNOPSIS

**stop** \[**-s**\|**--source** *SOURCE*\] \[**-y**\|**--yes**\]
\[**--json**\] \[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Stop the cargoes, virtual machines, and jobs declared in a Statefile and its
recursive SubStates. Cargo and VM keys use each rendered Statefile's namespace;
job keys use their plain names. Resources and secrets have no stop operation
and are skipped.

Uses the same per-item progress rows and operation summary as state apply and
state remove. Already stopped workloads appear as Unchanged. Missing workloads
and failed stop operations are reported as failures with a nonzero exit code.

The Statefile is rendered with its template arguments without creating
namespaces or executing command secrets. The rendered states are shown for
confirmation unless --yes is supplied.

Job stops use the existing daemon operation. A running job can fail to stop
while its execution task holds the job lock; that failure is reported.
Stopping a scheduled job preserves its schedule, so it may run again later.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*\
Path or URL to the Statefile. Uses the same default source lookup as state apply.

**-y**, **--yes**\
Skip the confirmation prompt.

**--json**

Emit newline-delimited JSON (NDJSON) using the state apply/remove record format
with operation "stop". Requires **-y**/**--yes**. Includes state and item
progress records and a final result record. Failed stops produce a failed
result and a nonzero exit code.

**-h**, **--help**\
Print help.

\[*ARGS*\]\
Additional arguments to pass to the Statefile template.

## EXAMPLES

```sh
nanocl state stop -s Statefile.yml
nanocl state stop -ys Statefile.yml -- --name example
nanocl state stop -ys Statefile.yml --json
```
