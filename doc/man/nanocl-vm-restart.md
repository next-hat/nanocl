---
title: Vm restart
---

# Vm restart

# NAME

restart - Shut down and start VMs by canonical keys

## SYNOPSIS

**restart** \[**--timeout** *SECONDS*\] \[**-f**\|**--force**\] \[**-h**\|**--help**\] *KEYS*...

## DESCRIPTION

Ask each guest to shut down through QMP, wait for its runtime to exit, then start it. A shutdown timeout reports failure without forced power-off or restart. Requires the daemon Unix socket or verified mutual TLS.

## OPTIONS

**--timeout** *SECONDS*
Guest shutdown grace period from 1 through 3600 seconds. Default: 60.

**-f**, **--force**
Immediately power off before starting again. Unsaved guest writes may be lost.

**-h**, **--help**
Print help

*KEYS*...
Canonical keys of the VMs to restart
