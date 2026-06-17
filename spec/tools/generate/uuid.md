# Tools: Generate: UUID

Subscrate vix-uuid-tool.

- menu "Tools"
  - submenu "Generate"
    - submenu "UUID"
      - menuitem "1" -> Generate UUID v1 using a timestamp, a monotonic counter, and a MAC address.
      - menuitem "2" -> Generate UUID v2 for DCE Security.
      - menuitem "3" -> Generate UUID v3 deterministic ID using MD5 hash of name and namespace.
      - menuitem "4" -> Generate UUID v4 random.
      - menuitem "5" -> Generate UUID v5 deterministic ID using SHA-1 hash of name and namespace.
      - menuitem "6" -> Generate UUID v6 using a timestamp such that the IDs are physically sortable.
      - menuitem "7" -> Generate UUID v7 using Unix epoch timestamp combined with random data.
      - menuitem "8" -> Generate UUID v8 return "00000000-0000-0000-0000-000000000000".
