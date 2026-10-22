---
title: Vm stop
sidebar_position: 91
---

# Vm stop

# NAME

stop - Shut down VMs by canonical keys

## SYNOPSIS

**stop** \[**--timeout** *SECONDS*\] \[**-f**\|**--force**\] \[**-h**\|**--help**\] *KEYS*...

## DESCRIPTION

Ask each guest to shut down through QMP and wait for its runtime to exit. A timeout reports failure without forced power-off. Requires the daemon Unix socket or verified mutual TLS.

## OPTIONS

**--timeout** *SECONDS*
Guest shutdown grace period from 1 through 3600 seconds. Default: 60.

**-f**, **--force**
Immediately power off without guest shutdown. Unsaved guest writes may be lost.

**-h**, **--help**
Print help

*KEYS*...
Canonical keys of the VMs to stop
