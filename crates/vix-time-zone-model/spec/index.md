# Time Zone

Time-zone modeling: the IANA canonical zone table + the app-wide active zone.
Vix has one application-wide **active time zone**, used by features that show
times (e.g. the clock box). It is chosen from a filterable list of IANA zones and
persisted across sessions.

## Choosing a zone

**View → Time Zone** is a submenu listing every IANA zone, each labeled
`UTC±HH:MM  Name` and ordered by **UTC offset** then name. Selecting one sets the
active zone; the item dispatches `view.time_zone:<name>`.

The chosen zone's canonical name is saved in `settings.time_zone` (default
`"UTC"`) and re-applied at startup.

## Data and offsets

The model is the full IANA **canonical** zone list (one entry per name from the
system tz database `zone.tab`), each carrying its **standard** (non-DST) UTC
offset, a DST flag, and the standard-period abbreviation. The table is generated
from the system tz database (see `crates/vix-time-zone-model/spec/index.md` for the
regeneration script).

Offsets are standard time and do **not** shift with daylight saving — the model
is pure data with no tz-rule engine. DST-accurate offsets would require the
binary tz database at runtime and are out of scope. Times shown elsewhere (the
clock box) therefore use the standard offset.

## As implemented in Vix

`time_zone_model` owns the zone table (`ZONES`, `Zone`), offset formatting
(`format_offset`, `Zone::offset_label`), and the active-zone state (`set_active`,
`active`, `active_name`, `active_offset_minutes`), mirroring the theme model. The
host builds the View → Time Zone submenu from `ZONES` (sorted by offset then
name) and applies a chosen zone by name (`set_time_zone_by_name`), persisting it.
See `crates/vix-time-zone-model/spec/index.md`.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Offsets are split with `unsigned_abs`, `checked_div` and `checked_rem`; the active-zone lookup uses `get`. `i32::MIN` minutes used to overflow in `abs`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `TZ-F1` | A division or remainder by 60 fails | `0` | The divisor is a constant above zero, so this cannot happen. |
| `TZ-F2` | The stored zone index is outside the table | UTC | The setter only stores valid indexes; if the table shrank, UTC is the neutral zone. |
