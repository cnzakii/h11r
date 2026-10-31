#![no_main]

mod common;

use common::{MAX_EVENTS, Outcome, Terminal, connection, finish, poll, seed_wire};
use h11r::Role;
use libfuzzer_sys::fuzz_target;

fn run(role: Role, control: u8, wire: &[u8], chunk_size: usize) -> Outcome {
    let mut connection = connection(role, control);
    let mut events = Vec::new();
    let mut body = Vec::new();
    let mut terminal = Terminal::Complete;

    for chunk in wire.chunks(chunk_size) {
        connection.receive_data(chunk).unwrap();
        terminal = poll(
            &mut connection,
            &mut events,
            &mut body,
            wire.len().saturating_mul(2).saturating_add(MAX_EVENTS),
        );
        if matches!(terminal, Terminal::Error { .. }) {
            break;
        }
    }
    if matches!(terminal, Terminal::Complete) {
        connection.receive_data(&[]).unwrap();
        terminal = poll(
            &mut connection,
            &mut events,
            &mut body,
            wire.len().saturating_mul(2).saturating_add(MAX_EVENTS),
        );
    }
    finish(&mut connection, events, body, terminal)
}

/// Delivers every byte and EOF before the first poll.
fn run_eof_first(role: Role, control: u8, wire: &[u8]) -> Outcome {
    let mut connection = connection(role, control);
    connection.receive_data(wire).unwrap();
    connection.receive_data(&[]).unwrap();
    let mut events = Vec::new();
    let mut body = Vec::new();
    let terminal = poll(
        &mut connection,
        &mut events,
        &mut body,
        wire.len().saturating_mul(2).saturating_add(MAX_EVENTS),
    );
    finish(&mut connection, events, body, terminal)
}

fuzz_target!(|input: &[u8]| {
    let Some((&control, wire)) = input.split_first() else {
        return;
    };
    let role = if control & 0x40 == 0 {
        Role::Server
    } else {
        Role::Client
    };
    let wire = if wire.is_empty() {
        seed_wire(role, control)
    } else {
        wire
    };
    let whole = run(role, control, wire, wire.len().max(1));
    let fragmented = run(role, control, wire, usize::from(control & 0x1f) + 1);

    let eof_first = run_eof_first(role, control, wire);

    for other in [&fragmented, &eof_first] {
        assert_eq!(whole.terminal, other.terminal);
        if whole.terminal == Terminal::Complete {
            assert_eq!(whole.events, other.events);
            assert_eq!(whole.body, other.body);
            assert_eq!(whole.states, other.states);
            assert_eq!(whole.trailing, other.trailing);
        }
    }
});
