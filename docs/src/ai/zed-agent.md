---
title: Orion Studio Agent
description: Use Orion Studio's native AI agent with Orion Studio-configured models, tools, profiles, skills, instructions, and MCP servers.
---

# Orion Studio Agent

Orion Studio Agent is Orion Studio's native agent path. It runs in the [Agent Panel](./agent-panel.md) and [Threads Sidebar](./parallel-agents.md#threads-sidebar), uses models configured through [LLM Providers](./llm-providers.md), and integrates with Orion Studio's project, editor, terminal, and review surfaces.

Use Orion Studio Agent when you want the agent to:

- read and search your project
- edit files
- run terminal commands
- use Orion Studio-managed MCP tools
- follow [Agent Profiles](./agent-profiles.md)
- use Orion Studio [Skills](./skills.md) and [Instructions](./instructions.md)
- show changes in Orion Studio's review UI

## What Orion Studio Agent Uses {#what-zed-agent-uses}

The section anchor and this page's filename retain their historical Zed names so incoming links continue to work.

| Capability                 | Source of truth                           |
| -------------------------- | ----------------------------------------- |
| Model access               | [LLM Providers](./llm-providers.md)       |
| Panel workflow             | [Agent Panel](./agent-panel.md)           |
| Tool availability          | [Agent Profiles](./agent-profiles.md)     |
| Tool approval behavior     | [Tool Permissions](./tool-permissions.md) |
| Built-in tools             | [Tools](./tools.md)                       |
| External tools             | [MCP](./mcp.md)                           |
| Reusable task instructions | [Skills](./skills.md)                     |
| Always-on instructions     | [Instructions](./instructions.md)         |

## How It Differs from Other Agent Paths {#other-agent-paths}

| Agent path                                | Main difference                                                                              |
| ----------------------------------------- | -------------------------------------------------------------------------------------------- |
| [Orion Studio Agent](./zed-agent.md)      | Uses Orion Studio's model, tool, profile, skill, instruction, and MCP configuration          |
| [External Agents](./external-agents.md)   | Use an ACP integration and often own auth, model, tool, and native instruction configuration |
| [Terminal Threads](./terminal-threads.md) | Run a CLI/TUI in a terminal-backed thread; the CLI owns auth and configuration               |

See [Agents](./agents.md) for the full comparison.
