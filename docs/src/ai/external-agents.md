---
title: External Agents - Orion Studio
description: Install and use ACP-integrated External Agents such as Claude, Codex, OpenCode, Copilot, Cursor, and Pi Coding Agent in Orion Studio.
---

# External Agents

External Agents are agents that integrate with Orion Studio through the [Agent Client Protocol (ACP)](https://agentclientprotocol.com). Orion Studio hosts the thread in the [Agent Panel](./agent-panel.md) and [Threads Sidebar](./parallel-agents.md#threads-sidebar), while the External Agent usually owns its own runtime, auth, model selection, tools, and native configuration.

Use [Terminal Threads](./terminal-threads.md) instead when you want to run a CLI or TUI directly in a terminal-backed thread.

External Agents run through their own process and provider relationship. Billing, legal terms, retention, and data handling are between you and the agent provider. Orion Studio does not charge for External Agents.

For Orion Studio-managed AI features and any operator-deployed Orion model service, see [AI Privacy](./privacy-and-security.md) and [Feedback and Training Data](./ai-improvement.md).

## Install from the ACP Registry {#registry}

The ACP Registry is the primary way to install common External Agents in Orion Studio.

Open the registry with {#action zed::AcpRegistry}, or open [Agent Settings](./agent-settings.md) with {#action agent::OpenSettings}, go to the **External Agents** page, click `Add Agent`, and choose `Install from Registry`.

After installation, the agent appears in the new-thread menu in the Agent Panel and Threads Sidebar.

## Common Agents {#common-agents}

Common External Agents include:

- Claude
- Codex
- OpenCode
- Copilot
- Cursor
- Pi Coding Agent

This list is curated, not exhaustive. Open the ACP Registry in Orion Studio for the current list of available agents.

For company-specific setup paths, including Claude, Codex, Gemini, OpenCode, Copilot, Cursor, Pi, and Poolside, see [AI by Company](./by-company.md).

## Claude Agent {#claude-agent}

Use Claude Agent when you want Claude running as an ACP-integrated External Agent in Orion Studio.

Install Claude Agent from the [ACP Registry](#registry), then start a Claude Agent thread from the Agent Panel or Threads Sidebar. Claude Agent owns its own authentication and billing. An Anthropic API key configured for [Orion Studio Agent](./zed-agent.md) does not automatically configure Claude Agent.

To choose your billing method, open a Claude Agent thread, run `/login`, and authenticate with an API key or with Claude Code where supported. Claude-specific files such as `CLAUDE.md` may be read by Claude Agent directly.

## Codex {#codex-cli}

Use Codex when you want Codex running as an ACP-integrated External Agent in Orion Studio.

Install Codex from the [ACP Registry](#registry), then start a Codex thread from the Agent Panel or Threads Sidebar. Codex owns its own authentication and billing. An OpenAI API key configured for [Orion Studio Agent](./zed-agent.md) does not automatically configure Codex.

Codex may support ChatGPT login, Codex API keys, OpenAI API keys, or Codex-native configuration depending on the installed version and environment. To change authentication, use the Codex thread's native login/logout flow.

## Gemini CLI {#gemini-cli}

Use Gemini CLI when you want Gemini running as an ACP-integrated External Agent in Orion Studio.

Install Gemini CLI from the [ACP Registry](#registry), then start a Gemini CLI thread from the Agent Panel or Threads Sidebar. Gemini CLI owns its own authentication and may prompt you to log in with Google, Vertex AI, or another Gemini-supported flow.

If `GEMINI_API_KEY` or `GOOGLE_AI_API_KEY` is available to the agent process, Gemini CLI uses that key. Otherwise, if you have configured an API key for Orion Studio's Google AI provider, Orion Studio passes that key to Gemini CLI as `GEMINI_API_KEY`.

## OpenCode {#opencode}

Use OpenCode when you want OpenCode running as an ACP-integrated External Agent in Orion Studio.

Install OpenCode from the [ACP Registry](#registry), then start an OpenCode thread from the Agent Panel or Threads Sidebar. OpenCode owns its own auth, model selection, and subscription behavior. To use OpenCode models in Orion Studio Agent instead, configure [OpenCode API access](./use-api-access.md#opencode).

## Copilot {#copilot}

Use Copilot External Agents where available when you want Copilot running as an ACP-integrated External Agent in Orion Studio.

Copilot agent auth is owned by the Copilot integration. To use Copilot Chat models in Orion Studio Agent or Copilot for edit prediction, see [Use an Existing Subscription](./use-an-existing-subscription.md#github-copilot).

## Cursor {#cursor}

Use Cursor External Agents where available when you want Cursor running as an ACP-integrated External Agent in Orion Studio.

Cursor subscriptions do not configure Orion Studio's LLM provider settings. Use Cursor's external-agent or CLI/TUI setup where available.

## Pi Coding Agent {#pi}

Use Pi Coding Agent when you want Pi running as an ACP-integrated External Agent in Orion Studio.

Pi is an agent harness, not an Orion Studio LLM subscription. Configure any provider auth, subscriptions, tools, or model choices in Pi.

## Poolside {#poolside}

Use Poolside when you want Poolside running as an ACP-integrated External Agent in Orion Studio.

Install Poolside from the [ACP Registry](#registry), then start a Poolside thread from the Agent Panel or Threads Sidebar. If you haven't authenticated yet, the thread offers `Log in to Poolside`, which runs `pool login` in a terminal. Authentication and model selection are configured through Poolside, not Orion Studio.

To configure Poolside from the terminal instead, install the [Poolside Agent CLI](https://github.com/poolsideai/pool), then run:

```sh
pool acp setup --editor zed
```

Poolside currently exposes `--editor zed` as its compatibility target. That command writes to the legacy `~/.config/zed/settings.json` path. Use it only when Orion Studio's legacy-config migration is enabled; otherwise, use the manual configuration below in the canonical Orion Studio settings file. Select `Poolside` from the new-thread menu after Orion Studio loads the setting.

To configure it manually, install the [Poolside Agent CLI](https://github.com/poolsideai/pool), make sure `pool` is on your `PATH`, then add it as a [Custom Agent](#custom-agents):

```json [settings]
{
  "agent_servers": {
    "Poolside": {
      "command": "pool",
      "args": ["acp"],
      "type": "custom"
    }
  }
}
```

Poolside's documentation still uses the historical [`/tools/zed` route](https://docs.poolside.ai/tools/zed). Treat its Zed names and paths as third-party compatibility instructions and map them to Orion Studio's canonical settings path where necessary.

## Start an External Agent Thread {#start-thread}

Open the [Agent Panel](./agent-panel.md), then use the agent selector or the new-thread menu to start a thread with an installed External Agent.

You can also create keybindings for specific agents with {#action agent::NewExternalAgentThread}.

## Configuration Boundaries {#configuration-boundaries}

External Agents run as separate processes that communicate with Orion Studio over ACP. This creates a boundary between Orion Studio configuration and agent-native configuration.

| Capability                       | Behavior in External Agent threads                                                                  |
| -------------------------------- | --------------------------------------------------------------------------------------------------- |
| Model/provider config            | Usually owned by the External Agent                                                                 |
| Auth/API keys/subscriptions      | Usually owned by the External Agent                                                                 |
| Orion Studio Agent profiles      | Do not apply unless the integration says otherwise                                                  |
| Orion Studio Skills              | Do not apply as Orion Studio Skills                                                                 |
| Native agent skills/instructions | Depends on the agent                                                                                |
| Orion Studio MCP servers         | May be forwarded over ACP                                                                           |
| Native MCP config                | May also be read by the agent                                                                       |
| Tool permissions                 | Orion Studio ACP/tool forwarding permissions may apply; native tool permissions depend on the agent |

For Orion Studio's native agent configuration, see [Orion Studio Agent](./zed-agent.md).

## Agent-Specific Auth and Config {#agent-auth-config}

External Agents may have their own sign-in flow, API key setup, subscription behavior, environment variables, and config files.

Examples:

- Claude Agent may use Claude Code auth and Claude-native config.
- Codex may use ChatGPT login, Codex API keys, OpenAI API keys, or Codex-native config.
- Cursor subscriptions do not configure Orion Studio's LLM provider settings; use Cursor's agent or CLI setup where available.
- Pi Coding Agent is an agent harness. Configure provider auth in Pi.

If an External Agent supports subscription-backed behavior, configure that in the agent unless the agent's Orion Studio integration says otherwise.

## Remote Projects {#remote-projects}

External Agents may read credentials locally, remotely, or through their own sign-in flow. Check the specific agent's setup path when using SSH, dev containers, or other remote projects.

Orion Studio LLM provider API keys saved in the local keychain are not automatically the same as an External Agent's credentials.

## Custom Agents {#custom-agents}

Use custom agents when you are developing an ACP-compatible agent or need to run an agent that is not in the registry.

Open [Agent Settings](./agent-settings.md), go to the **External Agents** page, click `Add Agent`, and choose `Add Custom Agent`. Orion Studio opens your settings file with an `agent_servers` entry.

```json [settings]
{
  "agent_servers": {
    "my-agent": {
      "type": "custom",
      "command": "node",
      "args": ["~/projects/agent/index.js", "--acp"],
      "env": {}
    }
  }
}
```

Registry-installed agents can also have per-agent settings under `agent_servers.<agent-id>`.

## Extension-Provided Agents {#extension-agents}

Extension-provided agents are deprecated. The [ACP Registry](#registry) is now the way to install agents, and previously installed extension agents are automatically migrated to their registry equivalents.

For details, see [Agent Server Extensions](../extensions/agent-servers.md).

## Importing Threads {#importing-threads}

Orion Studio can import existing threads from configured External Agents so they appear in your [Thread History](./agent-panel.md#multiple-threads) alongside the rest of your threads.

Open the Threads Sidebar with {#kb multi_workspace::ToggleWorkspaceSidebar}, then open Thread History by clicking the clock icon at the bottom of the sidebar or running {#action agents_sidebar::ToggleThreadHistory} from the Command Palette. Click **Import Threads**, choose the agents you want to import from, then click **Import Threads** again.

Orion Studio connects to each selected agent over ACP and adds sessions that are not already in your history. Imported threads are archived entries; open one to restore it and continue where you left off.

Only configured External Agents appear in the import dialog. Sessions without an associated working directory are skipped, and re-importing is safe because threads already in your history are skipped.

## MCP {#mcp}

Orion Studio-configured [MCP servers](./mcp.md) may be forwarded to External Agents over ACP. External Agents may also read their own native MCP configuration.

If an MCP tool does not appear in an External Agent, check both Orion Studio's MCP server configuration and the agent's native MCP configuration.

## Debugging {#debugging-agents}

Use {#action dev::OpenAcpLogs} from the Command Palette to inspect messages between Orion Studio and an External Agent.

Include ACP logs when reporting issues with External Agents.
