---
title: Model Context Protocol (MCP) in Orion Studio
description: Install and configure MCP servers in Orion Studio to extend your AI agent with external tools, data sources, and integrations.
---

# Model Context Protocol

Orion Studio uses the [Model Context Protocol](https://modelcontextprotocol.io/) to interact with context servers.

> The Model Context Protocol (MCP) is an open protocol for connecting LLM applications to external tools and data sources through a standard interface.

## Supported Features

Orion Studio currently supports MCP's [Tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) and [Prompts](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts) features.
We welcome contributions that help advance Orion Studio's MCP feature coverage (Discovery, Sampling, Elicitation, etc).

Orion Studio also handles the `notifications/tools/list_changed` notification from MCP servers. When a server adds, removes, or modifies its available tools at runtime, Orion Studio automatically reloads the tool list without requiring a server restart.

## Agent Path Support {#agent-path-support}

| Agent path                                | MCP behavior                                                                                     |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [Orion Studio Agent](./zed-agent.md)      | Uses Orion Studio-configured MCP servers directly                                                |
| [External Agents](./external-agents.md)   | Orion Studio can forward configured MCP servers over ACP; agents may also read native MCP config |
| [Terminal Threads](./terminal-threads.md) | Native CLIs/TUIs read their own MCP configuration                                                |

## Installing MCP Servers

### As Extensions

One of the ways you can use MCP servers in Orion Studio is by exposing them as an extension.
Check out the [MCP Server Extensions](../extensions/mcp-extensions.md) page to learn how to create your own.

Find MCP extensions in the app by opening the Command Palette and running {#action zed::Extensions}, or by opening **Settings → AI → MCP Servers**, clicking `Add Server`, and choosing `Install from Extensions`.

The available catalog depends on the extension registry configured for the build. A public Orion extension website is not assumed to be deployed. Some entries and repositories may retain Zed names because they come from the upstream extension ecosystem.

### As Custom Servers

Creating an extension is not the only way to use MCP servers in Orion Studio.
You can connect both local and remote MCP servers from **Settings → AI → MCP Servers** (also accessible via the {#action agent::OpenSettings} action, then selecting `MCP Servers`). Click `Add Server` in the page header, then choose `Add Local Server` or `Add Remote Server`. Your specified configuration will create entries in your settings file (which you can open with {#action zed::OpenSettingsFile}) similar to the ones below:

```json [settings]
{
  "context_servers": {
    "local-mcp-server": {
      "command": "some-command",
      "args": ["arg-1", "arg-2"],
      "env": {}
    },
    "remote-mcp-server": {
      "url": "https://example.com/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    },
    "remote-mcp-server-with-oauth": {
      "url": "https://mcp.example.com/mcp"
    }
  }
}
```

> Note: When a remote MCP server has no configured `"Authorization"` header, Orion Studio will prompt you to authenticate yourself against the MCP server using the standard MCP OAuth flow.

## Using MCP Servers

### Configuration Check

Most MCP servers require configuration after installation.

In the case of an extension, after installing it, Orion Studio will pop up a modal displaying what is required for you to properly set it up.
For example, the GitHub MCP extension requires you to add a [Personal Access Token](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens).

In the case of custom servers, make sure you check the provider documentation to determine what type of command, arguments, and environment variables need to be added to the JSON.

To check if your MCP server is properly configured, open **Settings → AI → MCP Servers** and watch the indicator dot next to its name.
If it's running correctly, the indicator will be green and its tooltip will say "Server is active".
If not, other colors and tooltip messages will indicate what is happening.

### Agent Panel Usage

Once installation is complete, you can return to the Agent Panel and start prompting.

How reliably MCP tools get called can vary from model to model.
Mentioning the MCP server by name can help the model pick tools from that server.

However, if you want to _ensure_ a given MCP server will be used, you can create [a custom profile](./agent-profiles.md) where all built-in tools (or the ones that could cause conflicts with the server's tools) are turned off and only the tools coming from the MCP server are turned on.

As an example, the Dagger team documents this pattern for its [Container Use MCP server](https://container-use.com/agent-integrations#zed). The `#zed` fragment is a third-party historical route, not Orion Studio branding:

```json [settings]
"agent": {
  "profiles": {
    "container-use": {
      "name": "Container Use",
      "tools": {
        "fetch": true,
        "copy_path": false,
        "find_path": false,
        "delete_path": false,
        "create_directory": false,
        "list_directory": false,
        "diagnostics": false,
        "read_file": false,
        "move_path": false,
        "grep": false,
        "edit_file": false,
        "terminal": false
      },
      "enable_all_context_servers": false,
      "context_servers": {
        "container-use": {
          "tools": {
            "environment_create": true,
            "environment_add_service": true,
            "environment_update": true,
            "environment_run_cmd": true,
            "environment_open": true,
            "environment_file_write": true,
            "environment_file_read": true,
            "environment_file_list": true,
            "environment_file_delete": true,
            "environment_checkpoint": true
          }
        }
      }
    }
  }
}
```

### Tool Permissions

> **Note:** In Orion Studio v0.224.0 and above, tool approval is controlled by `agent.tool_permissions.default`.
> In earlier versions, it was controlled by the `agent.always_allow_tool_actions` boolean (default `false`).

Orion Studio's Agent Panel provides the `agent.tool_permissions.default` setting to control tool approval behavior for the native Orion Studio agent:

- `"confirm"` (default) — Prompts for approval before running any tool action, including MCP tool calls
- `"allow"` — Auto-approves tool actions without prompting
- `"deny"` — Blocks all tool actions

For granular control over specific MCP tools, you can configure per-tool permission rules.
MCP tools use the key format `mcp:<server>:<tool_name>` — for example, `mcp:github:create_issue`.
The `default` key on a per-tool entry is the primary mechanism for MCP tools, since pattern-based rules match against an empty string for MCP tools and most patterns won't match.

Learn more about [how tool permissions work](./tool-permissions.md), how to further customize them, and other details.

### External Agents

MCP servers configured in Orion Studio are forwarded to [External Agents](./external-agents.md) via the [Agent Client Protocol](https://agentclientprotocol.com/). External Agents can also access MCP servers from their own native configuration files.

For details on what configuration is shared between Orion Studio and External Agents, see [Configuration Boundaries](./external-agents.md#configuration-boundaries).

### Error Handling

When an MCP server encounters an error while processing a tool call, the agent receives the error message directly and the operation fails.
Common error scenarios include:

- Invalid parameters passed to the tool
- Server-side failures (database connection issues, rate limits)
- Unsupported operations or missing resources

The error message from the context server will be shown in the agent's response, allowing you to diagnose and correct the issue.
Check the context server's logs or documentation for details about specific error codes.
