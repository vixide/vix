# vix-time-zone-chooser

Selection state for the **Time Zone chooser** overlay (Tools → Time Zone…).

There are hundreds of IANA zones, so the chooser is a filterable list rather than
a fixed dropdown.

## State

- `query` — the search text. Matching is a case-insensitive substring test
  against each zone's name **and** abbreviation.
- `matches()` — the matching indices into `vix_time_zone_model::ZONES`, in table
  order.
- `selected` — the highlighted row (index into `matches`).
- `scroll` — the viewport top, maintained by the host via `ensure_visible`.

## Behavior

- `open(active_name)` starts with an empty query (all zones) and highlights the
  active zone.
- `push(c)` / `backspace()` edit the query and re-filter, keeping the highlighted
  zone when it still matches (else snapping to the first match) and resetting
  scroll.
- `up` / `down` / `page_up` / `page_down` move the highlight (clamped, no wrap).
- `select(row)` sets the highlight from a mouse click.
- `selected_zone()` is the zone to apply on accept.

On accept the host calls `vix_time_zone_model::set_active(zone.name)`, persists the
name in settings, and closes the overlay. The crate is pure data; the host draws
the box (query line, scrollable list, scrollbar) and routes keys/mouse.
