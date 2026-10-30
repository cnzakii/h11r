# HTTP/1 fuzzing

The targets use public Rust APIs and separate four properties:

- `h11r_receive`: arbitrary peer bytes must not panic or loop without input.
- `h11r_fragmentation`: changing transport chunk boundaries, or receiving EOF
  before the first poll, must not change the accepted semantic message or
  error classification.
- `h11r_send`: valid request and response bytes emitted by one role must be
  accepted by the opposite role with the same body.
- `h11r_exchange`: across reused cycles, early responses, and complete sends,
  a server response head must announce `close` exactly when the connection
  will not persist.

CI runs a deterministic pull-request-sized budget for every target:

```console
make fuzz
```

Give a target a longer time budget when changing protocol parsing:

```console
cargo +nightly fuzz run h11r_fragmentation -- -max_total_time=600
```

One-byte inputs expand to protocol-shaped request or response seeds; longer
inputs are interpreted as raw peer bytes. Convert a confirmed minimized crash
into the smallest deterministic regression test.
