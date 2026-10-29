# Doc

This directory contains project documentation, generated man pages, and images
used by the documentation.

- [Multi-container Cargo RFC](multi-container-cargo-rfc.md)

## Lifecycle operation receipts

The following asynchronous API requests return the accepted operation UUID in
`X-Nanocl-Operation`. Paths below omit the API version prefix. Existing response
bodies and status codes are unchanged; a successful admission response does not
mean the runtime operation has finished.

| Request | Terminal event action |
| --- | --- |
| `POST /processes/cargo/{key}/start` or `POST /processes/vm/{key}/start` | `start` |
| `POST /processes/cargo/{key}/stop` | `stop` |
| `POST /processes/cargo/{key}/restart` | `restart` |
| `PATCH` or `PUT /cargoes/{key}` | `update` |
| `PATCH /cargoes/{key}/histories/{history_key}/revert` | `update` |
| `PATCH /vms/{key}` | `update` |
| `DELETE /cargoes/{key}` or `DELETE /vms/{key}` | `destroy` |

The CLI subscribes before sending the request, then waits for the same actor
kind, canonical key, and `Metadata.Operation` UUID. It accepts a normal terminal
event for the requested action or reports an error from that exact operation.
Terminal events also include `Metadata.SpecKey` and are published only after the
operation is durably settled. If settlement cannot be confirmed, the daemon sends
a correlated warning with action and reason `operation_pending` instead. The CLI
returns an interrupted, outcome-unknown error for that warning; it does not claim
the operation has completed or retry it. Inspect runtime and pending operation
state before deciding whether to retry. API clients implementing the same wait
must handle these warnings and use an unconditioned event subscription, since
name-only server conditions can close the stream on an unrelated operation.

Explicit Cargo and VM start/stop commands reach daemon admission even when a
previous status read would already match the requested state. Admission and
runtime verification decide the result; the CLI does not skip the request using
an unlocked status snapshot. Statefile reconciliation can still report an
unchanged, running specification as `Unchanged`.

For a healthchecked Cargo, `healthy` reports observed runtime readiness and can
precede the receipt-tagged terminal `start`. That terminal event confirms durable
operation settlement and lease release; readiness alone does not complete the
command. These notifications represent different stages.

Job start and stop acknowledge dispatch and display `Accepted`. VM stop and
restart commands use the synchronous `/vms/{key}/stop` and
`/vms/{key}/restart` endpoints and await their HTTP result.

An older daemon may accept a request without returning a valid receipt. The CLI
then reports that the request was accepted but completion cannot be correlated;
it does not infer success from another operation's event. Do not retry blindly:
inspect workload status and recent events before deciding whether to retry.
