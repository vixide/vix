# Tools: Generate: ZID

Subcrate vix-zid-tool.

- menu "Tools"
  - submenu "Generate"
    - submenu "ZID"
      - menuitem "128 bit = 32 hex" -> Generate secure random 128-bit 32-character hexadecimal lowercase string and insert it.
      - menuitem "256 bit = 64 hex" -> Generate secure random 256-bit 64-character hexadecimal lowercase string and insert it.
      - menuitem "512 bit = 128 hex" -> Generate secure random 512-bit 128-character hexadecimal lowercase string and insert it.
