---
title: Vm run
sidebar_position: 89
---

# Vm run

# NAME

run - Run a vm

## SYNOPSIS

**run** \[**-n**\|**--namespace**\] \[**--hostname**\] \[**--cloud-init**\] \[**--console**\] \[**--cpu**\] \[**--mem**\]
\[**--net-iface**\] \[**--user**\] \[**--password**\] \[**--ssh-key**\]
\[**--kvm**\] \[**-a**\|**--attach**\]
\[**-h**\|**--help**\] \<*NAME*\> \<*IMAGE*\>

## DESCRIPTION

Run a vm

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

**-a**, **--attach**
Attach to the vm

**-h**, **--help**
Print help

\<*NAME*\>
Name of the vm

\<*IMAGE*\>
Full path of the vm image
