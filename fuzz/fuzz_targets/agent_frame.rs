//! Fuzz the remote agent's frame codec (T717) against hostile bytes.
//!
//! Both ends of the agent protocol read bytes the other machine wrote: the
//! client reads whatever the remote host sends back over SSH, and the agent
//! reads whatever reaches its stdin. A lying length, a count past the end, a
//! nested batch or plain garbage must be refused, never panic or allocate
//! more than the input holds.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_authority::agent::wire;

fuzz_target!(|data: &[u8]| {
    // A byte stream of frames, as either side reads it.
    let _ = wire::decode_stream(data);

    // One payload: anything that decodes re-encodes to a payload that
    // decodes to the same message.
    if let Ok(message) = wire::decode(data) {
        let again = wire::encode(&message);
        assert_eq!(wire::decode(&again).ok(), Some(message));
    }
});
