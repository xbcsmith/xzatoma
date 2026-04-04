---
name: write_file
description: Write provided content to a named file in the output directory
license: MIT
compatibility: xzatoma >=0.1
allowed-tools:
  - write_file
---

# Write File Skill

Write the provided content to a file in the `tmp/output/` directory.

Use the filename specified by the user, or derive a descriptive filename from
the content if none is given.

Always write to `tmp/output/<filename>` relative to the current working
directory. Never write outside the `tmp/output/` directory.

Confirm the full file path after writing.
