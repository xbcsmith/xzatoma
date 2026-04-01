---
description: A skill that is missing the required name field in the frontmatter
---

# Invalid: Missing Name

This skill document is missing the required name field in the frontmatter. It
will be rejected during validation with a missing_name diagnostic.

## Expected Behavior

The skill scanner will parse this file, find that the name field is absent, and
record a MissingName invalid diagnostic. The skill will not appear in the valid
skill catalog.
