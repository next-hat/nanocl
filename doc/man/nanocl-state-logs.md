---
title: State logs
sidebar_position: 69
---

# State logs

# NAME

logs - Logs elements from a Statefile

## SYNOPSIS

**logs** \[**-s**\|**--source**\] \[**--since**\] \[**-u **\] \[**-t
**\] \[**--timestamps**\] \[**-f **\] \[**-h**\|**--help**\] \[*ARGS*\]

## DESCRIPTION

Logs elements from a Statefile

## OPTIONS

**-s**, **--source** *\<SOURCE\>*  
Path or Url to the Statefile

**--since** *\<SINCE\>*  
Only include logs since unix timestamp

**-u** *\<UNTIL\>*  
Only include logs until unix timestamp

**-t** *\<TAIL\>*  
If integer only return last n logs, if "all" returns all logs

**--timestamps**  
Bool, if set include timestamp to ever log line

**-f**  
Bool, if set open the log as stream

**-h**, **--help**  
Print help

\[*ARGS*\]  
Additional arguments to pass to the file
