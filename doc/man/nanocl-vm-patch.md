---
title: Vm patch
sidebar_position: 87
---

# Vm patch

# NAME

patch - Patch a vm

## SYNOPSIS

**patch** \[**--user**\] \[**--password**\] \[**--ssh-key**\]
\[**--hostname**\] \[**--cloud-init**\] \[**--console**\] \[**--cpu**\] \[**--mem**\] \[**--kvm**\]
\[**--net-iface**\] \[**-h**\|**--help**\] \<*KEY*\>

## DESCRIPTION

Patch a vm

## OPTIONS

**--cloud-init** *\<true|false\>*
Enable or disable cloud-init provisioning; omitted preserves the current setting. Networking uses DHCP independently. False generates and attaches no cloud-init seed.

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
