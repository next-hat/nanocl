---
title: State stats
sidebar_position: 72
---

# State stats

# NAME

stats - Show process statistics for workloads from a Statefile

## SYNOPSIS

**stats** \[**-s**\|**--source** *SOURCE*\] \[**--no-stream**\]
\[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Show statistics for the processes of cargoes, virtual machines, and jobs
declared in a Statefile. The combined table includes CPU %, MEM USAGE / LIMIT,
MEM %, NET I/O, BLOCK I/O, and PIDS. Resources, secrets, and namespace
declarations have no process statistics and are skipped.

The Statefile and its recursive SubStates are rendered once with the supplied
template arguments. Stats reads the resulting declarations without applying
changes, creating namespaces, or running Statefile secrets that execute
commands. Cargo and virtual machine keys use each rendered Statefile's
namespace; job keys use their plain names. Duplicate workload declarations are
queried once.

By default, stream statistics until Ctrl-C. Interactive terminal output is
cleared for each refresh unless TERM is dumb; redirected output appends plain
snapshots. Runtime streams attach once to the observed processes. Restart the
command after changing the Statefile or its template arguments, or to include
new processes.

With --no-stream, collect one statistics result per process and
print one combined table after all streams finish. Missing workloads or
workloads without processes may provide no samples. Read or stream errors are
reported, and the command exits with failure when the streams finish.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*\
Path or URL to the Statefile. Uses the same default source lookup as state apply.

**--no-stream**\
Disable streaming and print one combined snapshot.

**-h**, **--help**\
Print help

\[*ARGS*\]\
Additional arguments to pass to the Statefile template

## EXAMPLES

```sh
nanocl state stats -s Statefile.yml
nanocl state stats -s Statefile.yml --no-stream
nanocl state stats -s Statefile.yml --no-stream -- --name example
```
