# NAME

create - Create a vm

# SYNOPSIS

**create** \[**\--hostname**\] \[**\--cpu**\] \[**\--mem**\]
\[**\--net-iface**\] \[**\--user**\] \[**\--password**\]
\[**\--ssh-key**\] \[**\--kvm**\] \[**-h**\|**\--help**\] \<*NAME*\>
\<*IMAGE*\>

# DESCRIPTION

Create a vm

# OPTIONS

**\--hostname**=*HOSTNAME*

:   hostname of the vm

**\--cpu**=*CPU*

:   Cpu of the vm default to 1

**\--mem**=*MEMORY*

:   Memory of the vm in MB default to 512

**\--net-iface**=*NET_IFACE*

:   network interface of the vm

**\--user**=*USER*

:   Default user of the VM

**\--password**=*PASSWORD*

:   Default password of the VM

**\--ssh-key**=*SSH_KEY*

:   Ssh key for the user

**\--kvm**

:   Enable KVM

**-h**, **\--help**

:   Print help

\<*NAME*\>

:   Name of the vm

\<*IMAGE*\>

:   Name of the vm image
