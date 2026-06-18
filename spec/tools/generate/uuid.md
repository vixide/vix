# Tools: Generate: UUID

Subscrate uuid_tool.

- menu "Tools"
  - submenu "Generate"
    - submenu "UUID"
      - menuitem "1 = Unsortable Time Count MAC" -> Generate UUID v1 using a timestamp, a monotonic counter, and a MAC address.
      - menuitem "2 = DCE Security" -> Generate UUID v2 for DCE Security.
      - menuitem "3 = MD5 Name" -> Generate UUID v3 deterministic ID using MD5 hash of name and namespace.
      - menuitem "4 = Random" -> Generate UUID v4 random.
      - menuitem "5 = SHA-1 Name" -> Generate UUID v5 deterministic ID using SHA-1 hash of name and namespace.
      - menuitem "6 = Sortable Time Count MAC" -> Generate UUID v6 using a timestamp such that the IDs are physically sortable.
      - menuitem "7 = Time + Random" -> Generate UUID v7 using Unix epoch timestamp combined with random data.
      - menuitem "8 = Custom Placeholder" -> Generate UUID v8 return "00000000-0000-0000-0000-000000000000".
