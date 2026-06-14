# vix-time-zone-model

All time-zone modeling for Vix: the IANA zone table and the single
application-wide *active time zone*.

## Data model

`Zone` is one IANA canonical time zone:

- `name` — canonical IANA name (e.g. `America/New_York`).
- `std_offset_minutes` — the **standard** (non-DST) offset from UTC, in minutes.
- `has_dst` — whether the zone observes daylight saving at some point.
- `abbrev` — the standard-period abbreviation (e.g. `EST`; may be a numeric form
  like `+05`, or empty).

`ZONES` is the full canonical list (sorted by name). Offsets are **standard
time** — they do not shift with daylight saving, so this crate stays pure data
with no tz-rule engine. DST-accurate offsets would require the binary tz database
at runtime and are out of scope here.

## Active zone

Like the theme model holds the one active theme, this crate holds the one active
zone in process-global state. It defaults to **UTC** until set.

- `set_active(name) -> bool` — set by canonical name (false if unknown).
- `active() -> &Zone`, `active_name()`, `active_offset_minutes()` — read it.

UI crates (`vix-time-zone-chooser`) set the active zone; readers (the clock
panel, status bar) query it. The host persists the chosen name in settings and
restores it at startup.

## Helpers

- `format_offset(minutes) -> "UTC±HH:MM"`, and `Zone::offset_label()`.
- `index_of(name)`, `utc_index()`.

## Regenerating the table

`src/zones.rs` is generated from the system tz database. To regenerate (requires
Python 3.9+ with `zoneinfo` and a `/usr/share/zoneinfo/zone.tab`):

```python
import zoneinfo, datetime
names = set()
with open('/usr/share/zoneinfo/zone.tab') as f:
    for line in f:
        if line.startswith('#'):
            continue
        parts = line.rstrip('\n').split('\t')
        if len(parts) >= 3 and parts[2]:
            names.add(parts[2])
names.add('UTC')
jan = datetime.datetime(2025, 1, 15, 12)
jul = datetime.datetime(2025, 7, 15, 12)
for n in sorted(names):
    z = zoneinfo.ZoneInfo(n)
    dj, dl = jan.replace(tzinfo=z), jul.replace(tzinfo=z)
    oj = int(dj.utcoffset().total_seconds() // 60)
    ol = int(dl.utcoffset().total_seconds() // 60)
    zero = datetime.timedelta(0)
    std = oj if dj.dst() == zero else (ol if dl.dst() == zero else min(oj, ol))
    has_dst = oj != ol
    abbr = (dj.tzname() if dj.dst() == zero else dl.tzname()) or ''
    # emit: Zone { name, std_offset_minutes: std, has_dst, abbrev: abbr }
```

This crate is pure data with no dependencies and no I/O.
