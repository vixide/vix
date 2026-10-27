# Encoding

Text-file encodings: detect a file's encoding and BOM, decode it to text, and encode it back byte for byte.

**Status:** Shipped (T661). Built on `encoding_rs` (decoding and the legacy
encoders) and `chardetng` (guessing an unlabeled legacy file), both safe Rust.

## Why this crate exists

Before T661 every file Vix opened was assumed to be UTF-8: anything else was
decoded lossily and opened read-only, and UTF-16 was refused as binary for its
NUL bytes. CJK and Windows users meet Shift_JIS, GBK, EUC-KR and windows-125x
files every day. Detection, decoding and the save-side encode belong together,
so a file is always written back the way it was read; `vix-fileops` (reads)
and `vix-editor` (saves) both call this crate.

## API

- `TextEncoding` is an encoding plus whether the file has a BOM. `UTF8` (no
  BOM) is the default for new files. `from_label` takes any WHATWG label
  (`"Shift_JIS"`, `"latin1"`, …), with a trailing ` BOM` for a BOM. UTF-16
  always carries a BOM, since without one it can't be told from binary.
  `Display` gives the label back (`"UTF-16LE BOM"`, `"windows-1252"`).
- `CHOICES` lists the 16 labels the Reopen/Save with Encoding choosers offer:
  UTF-8 (with and without a BOM), UTF-16LE/BE, windows-1252/1250/1251,
  ISO-8859-2/15, KOI8-R, GBK, gb18030, Big5, Shift_JIS, EUC-JP and EUC-KR.
  `ISO-8859-1` decodes as windows-1252, as in every browser.
- `detect(bytes)` picks the encoding: a BOM wins, then valid UTF-8, then a
  `chardetng` guess. A guess counts only if it decodes with no malformed
  sequences **and** re-encodes to exactly the same bytes. `None` otherwise, and
  the caller falls back to today's lossy, read-only open, so detection can never
  silently corrupt a file.
- `decode_exact(bytes, enc)` is that lossless check for one encoding;
  `decode_lossy(bytes, enc)` (Reopen with Encoding) replaces what doesn't
  decode with U+FFFD and says whether it had to.
- `encode(text, enc)` writes the BOM, then the text. A character `enc` can't
  hold returns `Unmappable { ch, encoding }`; nothing is ever written as `?` or
  an HTML entity. UTF-16 is encoded by hand, because `encoding_rs`'s encoders
  never emit UTF-16 (per the WHATWG spec they fall back to UTF-8).
- `sniff_bom(bytes)` names the encoding a BOM at the start of `bytes` declares.

ISO-2022-JP is not offered: it is stateful and `chardetng` is told not to guess it.

## Consumers

- `vix-fileops::read_text_guarded` detects and decodes, sniffing the BOM before
  its NUL-byte binary check (and refusing a "UTF-16" file whose decoded text
  still has NULs). `write_text_in_file_encoding` writes text back in a file's
  existing encoding, for replace across files.
- `vix-editor`'s `Tab::encoding` remembers the encoding; `save_active` encodes
  with it, and `reopen_active_with_encoding` re-decodes the file.
- The App shell's **File → Reopen with Encoding…** / **Save with Encoding…**
  choosers (`file.reopen_with_encoding`, `file.save_with_encoding`) and the
  status bar's encoding segment.

## Tests

Byte-exact round trips for every `CHOICES` entry (with and without a BOM where
both exist), BOM detection for UTF-8/16LE/16BE, legacy guesses for Shift_JIS,
windows-1251, GBK and EUC-KR, unmappable characters, and lossy decodes.
