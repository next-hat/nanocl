# NAME

patch - Patch a vm

# SYNOPSIS

**patch** \[**\--user**\] \[**\--password**\] \[**\--ssh-key**\]
\[**\--hostname**\] \[**\--cpu**\] \[**\--mem**\] \[**\--kvm**\]
\[**\--net-iface**\] \[**-h**\|**\--help**\] \<*NAME*\>

# DESCRIPTION

Patch a vm

# OPTIONS

**\--user**=*USER*

:   Default user of the VM

**\--password**=*PASSWORD*

:   Default password of the VM

**\--ssh-key**=*SSH_KEY*

:   Ssh key for the user

**\--hostname**=*HOSTNAME*

:   hostname of the vm

**\--cpu**=*CPU*

:   Cpu of the vm default to 1

**\--mem**=*MEMORY*

:   Memory of the vm in MB default to 512

**\--kvm**

:   Enable KVM

**\--net-iface**=*NET_IFACE*

:   network interface of the vm

**-h**, **\--help**

:   Print help

\<*NAME*\>

:   Name of the vm
