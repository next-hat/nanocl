---
title: Cargo exec
sidebar_position: 3
---

# Cargo exec

# NAME

exec - Execute a command inside a cargo

## SYNOPSIS

**exec** \[**-t**\|**--tty**\] \[**--detach-keys**\] \[**-e **\]
\[**--privileged**\] \[**-u **\] \[**-w**\|**--workdir**\]
\[**-h**\|**--help**\] \<*KEY*\> \[*COMMAND*\]

## DESCRIPTION

Execute a command inside a cargo

## OPTIONS

**-t**, **--tty**  
Allocate a pseudo-TTY

**--detach-keys** *\<DETACH_KEYS\>*  
Override the key sequence for detaching a container

**-e** *\<ENV\>*  
Set environment variables

**--privileged**  
Give extended privileges to the command

**-u** *\<USER\>*  
Username or UID (format: "\<name\|uid\>\[:\<group\|gid\>\]")

**-w**, **--workdir** *\<WORKING_DIR\>*  
Working directory inside the container

**-h**, **--help**  
Print help

\<*KEY*\>
Canonical key of the cargo in which to execute the command

\[*COMMAND*\]  
Command to execute
