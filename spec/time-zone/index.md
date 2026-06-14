# Time Zone

Vix has one application-wide **active time zone**, used by features that show
times (e.g. the clock box). It is chosen from a filterable list of IANA zones and
persisted across sessions.

## Choosing a zone

**View → Time Zone…** opens a filterable chooser:

- Type to filter by a case-insensitive substring of the zone name or its
  abbreviation.
- Rows are ordered by **UTC offset** (the offset is the leftmost column), then by
  name.
- **↑ / ↓** and **PageUp / PageDown** move the highlight; the list scrolls with a
  one-character scrollbar (click or drag it, or use the mouse wheel).
- **Enter** or a **row click** sets the active zone and closes; **Esc** cancels.

The chosen zone's canonical name is saved in `settings.time_zone` (default
`"UTC"`) and re-applied at startup.

## Data and offsets

The model is the full IANA **canonical** zone list (one entry per name from the
system tz database `zone.tab`), each carrying its **standard** (non-DST) UTC
offset, a DST flag, and the standard-period abbreviation. The table is generated
from the system tz database (see `vix-time-zone-model/spec/index.md` for the
regeneration script).

Offsets are standard time and do **not** shift with daylight saving — the model
is pure data with no tz-rule engine. DST-accurate offsets would require the
binary tz database at runtime and are out of scope. Times shown elsewhere (the
clock box) therefore use the standard offset.

## As implemented in Vix

`vix-time-zone-model` owns the zone table (`ZONES`, `Zone`), offset formatting
(`format_offset`, `Zone::offset_label`), and the active-zone state (`set_active`,
`active`, `active_name`, `active_offset_minutes`), mirroring the theme model.
`vix-time-zone-chooser` holds the filterable selection state (query, matches,
scroll, highlight). The host wires the `view.time_zone` action, key/mouse, the
`draw_time_zone_chooser` overlay, and persistence. See
`vix-time-zone-chooser/spec/index.md`.
