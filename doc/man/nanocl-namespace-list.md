---
title: Namespace list
sidebar_position: 42
---

# Namespace list

# NAME

list - List existing namespaces

## SYNOPSIS

**list** \[**-q**\|**--quiet**\] \[**-l**\|**--limit**\]
\[**-o**\|**--offset**\] \[**--filters**\] \[**--wide**\] \[**-h**\|**--help**\]

## DESCRIPTION

List existing namespaces

By default, the compact table shows AGE in seconds (`s`), minutes (`m`),
hours (`h`), or days (`d`). Use `--wide` to show the exact creation timestamp.

## OPTIONS

**-q**, **--quiet**  
Only show keys

**-l**, **--limit** *\<LIMIT\>*  
Limit the number of results default to 100

**-o**, **--offset** *\<OFFSET\>*  
Offset the results to navigate through the results

**--filters** *\<FILTERS\>*  
Filters

**--wide**\
Show all columns and exact creation timestamps

**-h**, **--help**  
Print help
