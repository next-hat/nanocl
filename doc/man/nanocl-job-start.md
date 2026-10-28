---
title: Job start
sidebar_position: 33
---

# Job start

# NAME

start - Start a job

## SYNOPSIS

**start** \[**-h**\|**--help**\] \[*NAMES*\]

## DESCRIPTION

Start a job

The command acknowledges run admission. Inspect `nanocl job runs NAME` for
the execution outcome. Starting a Job with an active or unresolved run returns
a conflict; it does not enqueue another execution.

## OPTIONS

**-h**, **--help**  
Print help

\[*NAMES*\]  
