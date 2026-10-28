# HL7 v2 (vix-hl7)

HL7 v2 (ER7, pipe-and-hat) message parser, viewer model and MLLP framing, with an
optional blocking TCP sender. Editor actions `tools.hl7_view` and
`tools.hl7_send`.

## Why it is shaped this way

HL7 v2 messages are the lingua franca of hospital integration, and they carry
protected health information (PHI). Three rules follow and bind every part of
this crate and its App glue:

1. **No PHI at rest or in logs.** Nothing here logs, caches or writes a
   message, a reply or a field value. Errors carry error *kinds* only. The ACK
   tab is a read-only, never-saved tab. The only remembered state is the last
   `host:port`, held in memory for the session and never written to disk.
2. **Sending is an outward network action.** It is gated by the `http`
   capability (action id `tools.hl7_send`, see `spec/cargo-features`); it dials
   only a `host:port` the user types for that send (the port is required, there
   is no default and no saved target); and the prompt says the transport is
   plaintext MLLP. Nothing is ever sent automatically.
3. **Never panic on input.** Messages arrive from files, clipboards and sockets,
   so the parser is lenient and total.

## Pure core

- `Message::parse(&str) -> Message` never fails. Segments are separated by
  `\r`, `\n` or `\r\n`; blank lines are skipped; MLLP framing bytes pasted into
  the text are dropped. The delimiters come from `MSH` (`MSH-1` the field
  separator, `MSH-2` the encoding characters, any missing one defaulting to
  `|^~\&`); a message with no `MSH` parses with the defaults. `Message::has_msh`
  says whether the text looks like HL7 at all.
- The tree is `Message > Segment > Field > Repetition > Component >
  subcomponent`. Values stay *raw* (still escaped), so `Message::to_er7`
  re-encodes losslessly with `\r` separators; `unescape`/`escape` decode and
  encode `\F\ \S\ \T\ \R\ \E\ \.br\ \Xhh..\` (unknown or unterminated sequences
  are kept literally). `MSH` numbering counts the separator as `MSH-1`, as the
  standard does.
- `Path` is a 1-based address: `PID-5`, `PID-5.1`, `PID-5.1.2`, `PV1-3(2).1`
  (repetition in parentheses). `Message::get` / `get_raw` resolve one.
- `rows(&Message) -> Vec<Row>` is the segment-per-line view: each segment, then
  its non-empty fields numbered `PID-5`, with nested rows (`PID-5.1`,
  `PID-3(2).1`, `PID-3.4.2`) for a field with components, repetitions or
  subcomponents. Each row carries the source `line` and `col` for jumping.
  `pretty` renders the rows as text.
- `Ack::from_message` reads `MSA-1` (`AA/AE/AR/CA/CE/CR` -> `AckCode`, with
  `is_accept`/`is_nak`), `MSA-2`, `MSA-3` and the first `ERR` code.
- `Panel` is the overlay's state: all rows, a case-insensitive filter over label
  and value, and a highlight/scroll over the filtered rows via `vix-list-state`.
  `Panel::open(msg, cursor_line)` starts on the segment at the cursor.

## MLLP (`mllp`, pure)

`frame(payload)` wraps in `0x0B ... 0x1C 0x0D`; `to_wire(text)` normalizes
editor text to the `\r`-separated wire form (dropping blank lines and stray
framing bytes). `Deframer::push(bytes)` extracts whole payloads incrementally,
discarding noise between frames, restarting on a stray start byte and returning
`MllpError::TooLarge` past its limit.

## Sender (`net`, feature `net`)

Off by default so a build without the `http` capability has no socket code from
this crate (`vix-tui`'s `http` feature turns it on). `Endpoint::parse` accepts
`host:port` and `[v6]:port` and refuses a missing or zero port
(`EndpointError`). `send(&Endpoint, payload, &SendOptions)` resolves, connects
with a per-address timeout, writes one framed message and reads one framed reply
under read/write timeouts and an overall reply deadline, with a reply size cap.
Errors are the typed `SendError` (`Resolve`, `Connect`, `Timeout`, `Write`,
`Read`, `Closed`, `Framing`), which carry `io::ErrorKind` only and expose a
stable `code()` the App maps to a translated message.

## App glue (`tui/src/app/hl7.rs`, `tui/src/ui/hl7.rs`)

- **Tools -> HL7 View** (`tools.hl7_view`): parse the active buffer, refuse one
  with no `MSH`, otherwise open the overlay (`App::hl7_panel`). Type to filter,
  Up/Down/PageUp/PageDown move, Enter (or a click) jumps the cursor to the
  segment/field and closes, Esc closes.
- **Tools -> HL7 Send** (`tools.hl7_send`, capability `http`): refuse a buffer
  with no `MSH`; prompt for `host:port` (pre-filled with this session's last
  one); on Enter validate the endpoint, then send on a background thread. The
  outcome comes back over a channel polled each event-loop tick (`poll_hl7`, the
  same shape as `poll_http`). A reply opens in a **read-only** tab titled with
  the endpoint, holding an acknowledgement summary line and the numbered view of
  the reply; the status line shows the ack code. A failure shows a translated
  error and opens no tab.

## Tests

Unit tests in each module; `crates/vix-hl7/tests/robustness.rs` (proptest: parse/escape/path
never panic, ER7 and MLLP round-trip); `crates/vix-hl7/tests/loopback.rs` (feature `net`: an
in-test server thread on `127.0.0.1` acknowledges, rejects, hangs up and stalls);
`tui/tests/integration/hl7.rs` (the overlay and the whole send flow through the
real `App`).

## Not done

TLS-wrapped MLLP, a long-lived listener (receiving messages), batch (`FHS/BHS`)
files, per-version segment/field dictionaries (the view shows numbers, not
field names) and message editing from the view are out of scope.
