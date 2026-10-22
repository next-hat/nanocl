---
title: Vm resize
---

# Vm resize

# NAME

resize - Grow a running VM without restarting it

## SYNOPSIS

**resize** *KEY* \[**--cpu** *COUNT*\] \[**--memory** *MIB*\] \[**--disk-size** *BYTES*\]

## DESCRIPTION

Request total resource capacities through the private QMP connection. Supply at least one option. CPU and memory cannot exceed the ceilings reserved at boot. Memory increases use 128 MiB multiples; disk sizes use 512-byte alignment. Only growth is allowed, with equal totals accepted for persistence recovery.

The VM must be running with a resize-capable runtime. Access requires the daemon Unix socket or verified mutual TLS. The command prints QEMU capacity and persistence as JSON. Guest CPU/memory onlining and partition/filesystem growth remain guest operations.

Changes are not atomic. After an error or timeout, run `vm resources` before retrying: some changes may have succeeded. If `Persistent` is false, retry the desired CPU/memory totals and confirm persistence before restarting. See [live resource growth](../vm-runtime.md#live-resource-growth) for runtime and storage limitations.

## OPTIONS

**--cpu** *COUNT*
Requested total vCPU count, up to the boot-time `MaxCpu`.

**--memory** *MIB*
Requested total QEMU memory in MiB, up to the boot-time `MaxMemory`.

**--disk-size** *BYTES*
Requested total boot disk size in bytes. Does not grow guest partitions or filesystems.

**-h**, **--help**
Print help.

*KEY*
Canonical VM key in `namespace.name` form.
