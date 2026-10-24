---
title: Vm run
sidebar_position: 89
---

# Vm run

# NAME

run - Run a vm

## SYNOPSIS

**run** \[**-n**\|**--namespace**\] \[**--hostname**\] \[**--cloud-init**\] \[**--cloud-init-user-data**\] \[**--console**\] \[**--cpu**\] \[**--mem**\] \[**--max-cpu**\] \[**--max-memory**\]
\[**--net-iface**\] \[**--user**\] \[**--password**\] \[**--ssh-key**\]
\[**--kvm**\] \[**-a**\|**--attach**\]
\[**-h**\|**--help**\] \<*NAME*\> \<*IMAGE*\>

## DESCRIPTION

Run a vm

## OPTIONS

**--cloud-init** *\<true|false\>*
Enable cloud-init provisioning; defaults to true for compatibility. Networking uses DHCP independently. False generates and attaches no cloud-init seed.

**--cloud-init-user-data** *\<FILE\>*
Read complete user-data from a local UTF-8 file, at most 64 KiB without NUL bytes. The file replaces generated user/password/SSH/hostname provisioning; NanoCL retains the stable instance ID and DHCP network-config. Cloud-config, scripts, and MIME multipart text are passed unchanged. An empty file selects blank user-data. The content is retained but unused when cloud-init is disabled, and is visible to operators through the VM specification and Docker environment. The runtime must advertise `io.nanocl.vm-cloud-init-user-data=1`. See [custom cloud-init](../vm-runtime.md#custom-cloud-init-user-data) for structured Statefiles and first-boot semantics.

**--console** *\<true|false\>*
Enable the browser console; defaults to false. Access follows the daemon's configured operator access controls.

**-n**, **--namespace** *\<NAMESPACE\>*
Namespace for the new VM; defaults to global

**--hostname** *\<HOSTNAME\>*
hostname of the vm

**--cpu** *\<CPU\>*
Cpu of the vm default to 1

**--mem** *\<MEMORY\>*
Memory of the vm in MB default to 512

**--max-cpu** *COUNT*
Reserve a maximum vCPU count for live growth. Omitted uses the initial CPU count.

**--max-memory** *MIB*
Reserve maximum memory in MiB for live growth. Omitted uses the initial memory allocation. See [live resource growth](../vm-runtime.md#live-resource-growth).

**--net-iface** *\<NET_IFACE\>*
network interface of the vm

**--user** *\<USER\>*
Default user of the VM

**--password** *\<PASSWORD\>*
Default password of the VM

**--ssh-key** *\<SSH_KEY\>*
Ssh key for the user

**--kvm**
Enable KVM

**-a**, **--attach**
Attach to the vm

**-h**, **--help**
Print help

\<*NAME*\>
Name of the vm

\<*IMAGE*\>
Full path of the vm image
