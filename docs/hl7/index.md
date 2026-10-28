# HL7 v2 View & Send

Read an HL7 v2 message (pipe-and-hat, "ER7") as numbered segments and fields,
and send it to an interface engine over MLLP — all from the active buffer.

## Open a message

Put an HL7 v2 message in a buffer. Segments can be separated by newlines or by
the carriage returns HL7 files often use:

```text
MSH|^~\&|SEND|FAC|RECV|FAC|202401011200||ADT^A01|MSG0001|P|2.5
PID|1||12345^^^HOSP^MR||DOE^JANE^Q||19800101|F
```

The first line must be an `MSH` segment. Its second character is the field
separator and the next four are the component, repetition, escape and
subcomponent characters; Vix reads them from there, so unusual delimiters work.

## Tools → HL7 View

Opens an overlay with one row per segment and per non-empty field, numbered the
way the HL7 standard numbers them (`MSH-1` is the field separator itself):

```text
PID
  PID-3        12345^^^HOSP^MR
    PID-3.1      12345
    PID-3.4      HOSP
    PID-3.5      MR
  PID-5        DOE^JANE^Q
    PID-5.1      DOE
```

- Type to filter by field number or value.
- **Up / Down / PageUp / PageDown** move; **Enter** (or a click) jumps the
  cursor to that segment or field and closes the view; **Esc** closes it.
- The view opens on the segment the cursor is on.
- Escape sequences (`\F\`, `\S\`, `\.br\`, `\X41\`, …) are decoded in values.

The parser is lenient: a ragged or damaged message still shows whatever fields
it has, and nothing crashes on bad input.

## Tools → HL7 Send…

Sends the message to a server that speaks MLLP (the `0x0B … 0x1C 0x0D` framing
used by most HL7 v2 interfaces).

1. Choose **Tools → HL7 Send…**.
2. Type the target as `host:port`, e.g. `127.0.0.1:2575`. The port is required;
   there is no default and nothing is sent anywhere you did not type.
3. Press **Enter**. The send runs in the background; the editor stays usable.
4. The reply opens in a new **read-only** tab: a line such as
   `Acknowledgement AA for message MSG0001`, then the reply's numbered view. The
   status line shows the code (`AA`/`CA` accepted; `AE`/`AR`/`CE`/`CR` rejected).

Timeouts apply to connecting, writing and waiting for the reply, and a failure
(host not found, refused, timed out, closed early, malformed reply) is shown as
a message without opening a tab.

### Privacy and safety

- MLLP is **plaintext TCP**. Send only over networks you trust, or through a VPN
  or tunnel.
- Vix never logs, caches or saves a message or its reply. The reply tab cannot be
  saved over a file. The last `host:port` is remembered in memory for the
  session only.
- Sending is a network action, so it belongs to the `http` build capability: a
  build without it (for example `--no-default-features`) leaves **HL7 Send**
  out. **HL7 View** is always available.

See the crate specification at `crates/vix-hl7/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
