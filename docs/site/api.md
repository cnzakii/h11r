---
description: Public Python API reference for h11r.
---

# Python API

The Python package exposes one connection type, immutable protocol events and
collected bodies, actor roles and states, receive statuses, and protocol
errors.

API methods operate on protocol state only. Methods that send HTTP return bytes
for the caller to write; `receive_data()` accepts bytes already read by the
caller, while `receive_buffer()` optionally lends writable storage to a
compatible transport.

::: h11r.Connection
    options:
      group_by_category: false
      members:
        - receive_data
        - receive_buffer
        - next_event
        - collect_body
        - send_request
        - send_informational_response
        - send_response
        - send_data
        - send_data_parts
        - end_of_message
        - start_next_cycle
        - close

## Receive buffers

::: h11r.ReceiveBuffer
    options:
      heading_level: 3
      group_by_category: false

## Full-body collection

::: h11r.BodyCollector
    options:
      heading_level: 3
      group_by_category: false

::: h11r.CollectedBody
    options:
      heading_level: 3
      members: false

## Roles and states

::: h11r.Role
    options:
      heading_level: 3
      members: false

::: h11r.BodyTooLarge
    options:
      heading_level: 3
      members: false

::: h11r.State
    options:
      heading_level: 3
      members: false

::: h11r.ReceiveStatus
    options:
      heading_level: 3
      members: false

## Events

::: h11r.Request
    options:
      heading_level: 3
      members: false

::: h11r.InformationalResponse
    options:
      heading_level: 3
      members: false

::: h11r.Response
    options:
      heading_level: 3
      members: false

::: h11r.Data
    options:
      heading_level: 3
      members: false

::: h11r.EndOfMessage
    options:
      heading_level: 3
      members: false

::: h11r.ConnectionClosed
    options:
      heading_level: 3
      members: false

## Errors

::: h11r.ProtocolError
    options:
      heading_level: 3
      members: false

::: h11r.LocalProtocolError
    options:
      heading_level: 3
      members: false

::: h11r.RemoteProtocolError
    options:
      heading_level: 3
      members: false
