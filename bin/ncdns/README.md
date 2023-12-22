# Nanocl official controller dns

The official nanocl controller dns build on top of dnsmasq.

See [nanocl](https://github.com/next-hat/nanocl) for more informations.

## Overview

The the default nanocl controller for domain name is using dnsmasq.</br>
It will ensure each cargo instance will own a dns entry.</br>
The dns entry will be the cargo generated from the cargo key.</br>
We will replace `-` and `_` by a `.` and will be generated this way: `nanocl.<key>.local`</br>
This process should never stop by itself or by a crash.</br>
It will loop till it have a connection to nanocl daemon</br>
and be able to watch for his events.
