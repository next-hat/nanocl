---
title: Cargo list
sidebar_position: 7
---

# Cargo list

# NAME

list - List existing cargo

## SYNOPSIS

**list** \[**-n**\|**--namespace**\] \[**-q**\|**--quiet**\] \[**-l**\|**--limit**\]
\[**-o**\|**--offset**\] \[**--filters**\] \[**--wide**\] \[**-h**\|**--help**\]

## DESCRIPTION

List existing cargo

By default, the compact table shows AGE in seconds (`s`), minutes (`m`),
hours (`h`), or days (`d`). Use `--wide` to show exact creation and update timestamps.

## OPTIONS

**-n**, **--namespace** *\<NAMESPACE\>*
Optional namespace filter; omitted returns resources from all namespaces

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
