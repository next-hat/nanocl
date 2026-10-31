---
title: Vm
sidebar_position: 92
---

# Vm

# NAME

vm - Manage virtual machines

## SYNOPSIS

**vm** \[**-h**\|**--help**\] \<*subcommands*\>

## DESCRIPTION

Manage virtual machines. Guest networking uses DHCP with the Docker endpoint IPv4 address. Cloud-init provisioning is optional and enabled by default for compatibility. The browser console is disabled by default and follows the daemon operator trust boundary.

See [VM runtime and migration](../vm-runtime.md) for the network model, local runtime build, migration, and validation instructions.

## OPTIONS

**-h**, **--help**
Print help

# SUBCOMMANDS

vm-run(1)
Run a vm

vm-create(1)
Create a vm

vm-list(1)
List vms

vm-remove(1)
Remove VMs by canonical keys

vm-inspect(1)
Inspect a VM by canonical key

vm-history(1)
List specification revisions or inspect one revision by canonical key

vm-revert(1)
Restore a specification revision by canonical key

vm-start(1)
Start VMs by canonical keys

vm-stop(1)
Shut down VMs by canonical keys

vm-restart(1)
Shut down and start VMs by canonical keys

vm-status(1)
Query actual QEMU execution state

vm-resize(1)
Grow running CPU, memory, or boot disk resources

vm-resources(1)
Query QEMU capacity, boot ceilings, and CPU/memory persistence

vm-attach(1)
Attach to a vm

vm-patch(1)
Patch a vm

vm-help(1)
Print this message or the help of the given subcommand(s)
