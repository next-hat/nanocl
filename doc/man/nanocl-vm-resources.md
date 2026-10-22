---
title: Vm resources
---

# Vm resources

# NAME

resources - Query QEMU capacity and persistence

## SYNOPSIS

**resources** *KEY*

## DESCRIPTION

Print JSON containing QEMU's current `Cpu`, `Memory` in MiB, boot `DiskSize` in bytes, boot-time `MaxCpu` and `MaxMemory`, and `Persistent`. The VM must be running. Access requires the daemon Unix socket or verified mutual TLS.

The values describe QEMU devices, not guest-online CPU/memory or filesystem capacity. `Persistent` confirms that observed CPU and memory match both the saved specification and runtime restart configuration. After a resize failure, query these values before deciding whether to retry. If persistence is false, retry the desired CPU/memory totals before restarting.

## OPTIONS

**-h**, **--help**
Print help.

*KEY*
Canonical VM key in `namespace.name` form.
