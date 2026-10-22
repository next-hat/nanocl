---
title: Vm create
sidebar_position: 78
---

# Vm create

# NAME

create - Create a vm

## SYNOPSIS

**create** \[**-n**\|**--namespace**\] \[**--hostname**\] \[**--cloud-init**\] \[**--console**\] \[**--cpu**\] \[**--mem**\]
\[**--net-iface**\] \[**--user**\] \[**--password**\] \[**--ssh-key**\]
\[**--kvm**\] \[**-h**\|**--help**\] \<*NAME*\> \<*IMAGE*\>

## DESCRIPTION

Create a vm

## OPTIONS

**--cloud-init** *\<true|false\>*
Enable cloud-init provisioning; defaults to true for compatibility. Networking uses DHCP independently. False generates and attaches no cloud-init seed.

**--console** *\<true|false\>*
Enable the browser console; defaults to false. Access requires the daemon Unix socket or verified mutual TLS.

**-n**, **--namespace** *\<NAMESPACE\>*
Namespace for the new VM; defaults to global

**--hostname** *\<HOSTNAME\>*
hostname of the vm

**--cpu** *\<CPU\>*
Cpu of the vm default to 1

**--mem** *\<MEMORY\>*
Memory of the vm in MB default to 512

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

**-h**, **--help**
Print help

\<*NAME*\>
Name of the vm

\<*IMAGE*\>
Full path of the vm image
