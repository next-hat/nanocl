---
title: Vm patch
sidebar_position: 87
---

# Vm patch

# NAME

patch - Patch a vm

## SYNOPSIS

**patch** \[**--user**\] \[**--password**\] \[**--ssh-key**\]
\[**--hostname**\] \[**--cloud-init**\] \[**--cloud-init-user-data**\] \[**--console**\] \[**--cpu**\] \[**--mem**\] \[**--kvm**\]
\[**--net-iface**\] \[**-h**\|**--help**\] \<*KEY*\>

## DESCRIPTION

Patch a vm

## OPTIONS

**--cloud-init** *\<true|false\>*
Enable or disable cloud-init provisioning; omitted preserves the current setting. Networking uses DHCP independently. False generates and attaches no cloud-init seed.

**--cloud-init-user-data** *\<FILE\>*
Replace custom user-data with a local UTF-8 file, at most 64 KiB without NUL bytes. Omitted preserves current user-data; an empty file selects blank user-data. Custom data replaces generated user/password/SSH/hostname provisioning while NanoCL retains the stable instance ID and DHCP network-config. It is stored but unused when cloud-init is disabled, and is visible through the VM specification and Docker environment. Requires `io.nanocl.vm-cloud-init-user-data=1` on the runtime. Changing user-data does not force first-boot cloud-init modules to rerun on an existing guest disk. See [custom cloud-init](../vm-runtime.md#custom-cloud-init-user-data).

**--console** *\<true|false\>*
Enable or disable the browser console; omitted preserves the current setting. Access requires the daemon Unix socket or verified mutual TLS.

**--user** *\<USER\>*  
Default user of the VM

**--password** *\<PASSWORD\>*  
Default password of the VM

**--ssh-key** *\<SSH_KEY\>*  
Ssh key for the user

**--hostname** *\<HOSTNAME\>*  
hostname of the vm

**--cpu** *\<CPU\>*  
Number of CPUs; omitted preserves the current setting

**--mem** *\<MEMORY\>*  
Memory in MB; omitted preserves the current setting

**--kvm**  
Enable KVM

**--net-iface** *\<NET_IFACE\>*  
network interface of the vm

**-h**, **--help**  
Print help

\<*KEY*\>
Canonical key of the VM
