---
title: Job stop
---

# Job stop

## NAME

stop - Stop the current Job execution

## SYNOPSIS

**nanocl job stop** *NAMES*...

## DESCRIPTION

Request stopping the current execution of each named Job and prevent later
steps from launching. The command acknowledges the control request; inspect
runs for the confirmed outcome. Retained executions remain available through
`nanocl job runs`. Runtime observation failures remain visible and do not
invent successful outcomes.

## EXAMPLES

```sh
nanocl job stop backup
nanocl job runs backup
```
