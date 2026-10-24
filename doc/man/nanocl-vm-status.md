---
title: Vm status
---

# Vm status

# NAME

status - Query actual QEMU execution state

## SYNOPSIS

**status** \[**-h**\|**--help**\] *KEY*

## DESCRIPTION

Print JSON with `Running` and `Status` from QEMU. A paused guest reports `Running: false` even though its runtime container is running. A stopped runtime reports `Status: exited`. Status remains available while a shutdown request waits through its grace period. Access follows the daemon's configured operator access controls.

## OPTIONS

**-h**, **--help**
Print help

*KEY*
Canonical VM key in `namespace.name` form
