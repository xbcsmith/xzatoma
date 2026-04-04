---
name: summarize
description: Condense provided text into a concise summary
license: MIT
compatibility: xzatoma >=0.1
allowed-tools: []
---

# Summarize Skill

Read the provided text carefully and produce a summary of three to five
sentences. Preserve all key points. Remove redundant detail and filler words.
Do not add information that is not present in the source text.

When writing a summary to a file, always use the write_file tool and write to
the path specified in the task. If no path is given, write to
tmp/output/summary.txt relative to the current working directory.
