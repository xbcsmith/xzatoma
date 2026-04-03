# MCP Tool Examples

This file documents the tools provided by the `demo-filesystem` MCP server and
gives example prompts for interactive use.

## Server: demo-filesystem

The `demo-filesystem` server provides filesystem access scoped to the
`tmp/output/` directory. All paths are relative to that root. The server is
started automatically by XZatoma when the demo plan runs.

## Available Tools

The exact tool names depend on the version of
`@modelcontextprotocol/server-filesystem` installed. Common tools include:

| Tool               | Description                                       |
| ------------------ | ------------------------------------------------- |
| `read_file`        | Read the contents of a file                       |
| `write_file`       | Write content to a file                           |
| `list_directory`   | List files and directories                        |
| `create_directory` | Create a new directory                            |
| `delete_file`      | Delete a file                                     |
| `move_file`        | Move or rename a file                             |
| `get_file_info`    | Get file metadata (size, permissions, timestamps) |
| `search_files`     | Search for files by name pattern                  |

## Example Prompts

Use these prompts after running `./setup.sh` to exercise MCP tools in an
interactive session:

    xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db chat

### List files in the output directory

    List all files in the output directory using the MCP filesystem server.

### Write a file

    Use the MCP filesystem server to write the text "hello from MCP" to a file
    named hello.txt in the output directory.

### Read a file back

    Use the MCP filesystem server to read the file hello.txt from the output
    directory and display its contents.

### Create a subdirectory and write into it

    Use the MCP filesystem server to create a subdirectory named notes inside
    the output directory, then write a file named notes/readme.txt containing
    the text "Created by MCP demo".

### Get file metadata

    Use the MCP filesystem server to get the file info for mcp_hello.txt in
    the output directory. Report the file size and last modified time.

### Search for files

    Use the MCP filesystem server to search for all .txt files in the output
    directory and list their names.

## Notes

- The MCP server is launched as a subprocess by XZatoma via `npx` on first use.
- The server process terminates when the XZatoma session ends.
- All file paths accepted by the server are resolved relative to
  `./tmp/output/`.
- Attempts to access paths outside `./tmp/output/` are rejected by the server.
