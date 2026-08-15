---
title: AI Privacy and Security
description: Understand Orion Studio AI request paths, provider boundaries, local controls, and deployment-dependent services.
---

# AI Privacy and Security

AI privacy depends on the path you configure. The Orion Studio source tree does
not by itself create a hosted privacy commitment, account service, or data
processing agreement.

| Path                                                        | Who receives requests                             | Primary privacy boundary                         |
| ----------------------------------------------------------- | ------------------------------------------------- | ------------------------------------------------ |
| [Provider API access](./use-api-access.md)                  | The configured provider                           | Provider API terms and your account settings     |
| [Existing subscriptions](./use-an-existing-subscription.md) | The subscription provider                         | Provider subscription terms                      |
| [Gateways](./use-a-gateway.md)                              | The gateway and upstream providers                | Both gateway and provider policies               |
| [Local or self-hosted models](./use-a-local-model.md)       | Your local or self-hosted endpoint                | Your machine and deployment controls             |
| [External Agents](./external-agents.md)                     | The agent and its providers                       | Agent, ACP, tool, and provider configuration     |
| [Terminal Threads](./terminal-threads.md)                   | The CLI or TUI and its providers                  | That tool's authentication and data policy       |
| [Orion-hosted models](../account/zed-hosted-models.md)      | The configured Orion operator and model providers | Operator policy; available only after deployment |

Provider keys saved through Orion Studio's provider UI are stored in the system
keychain rather than `settings.json`. Environment variables, external agents,
MCP servers, terminal commands, and project instruction files have separate
exposure paths.

## Tools and project trust

Agent tools can read files, edit code, run commands, fetch URLs, or call remote
systems. Review [Tool Permissions](./tool-permissions.md),
[Sandboxing](./sandboxing.md), [MCP](./mcp.md), and
[Worktree Trust](../worktree-trust.md) before authorizing untrusted content.
Sandboxing reduces risk but does not replace provider, operating-system, or
deployment security controls.

## Orion-hosted services

An operator must publish its model providers, retention, training policy,
subprocessors, data residency, abuse controls, deletion process, and security
contact before enabling Orion-hosted models. Do not infer upstream Zed provider
agreements or privacy promises for Orion Studio.

## Local controls

- Disable all AI features with `"disable_ai": true`.
- Keep telemetry disabled unless you trust a configured Orion endpoint; see
  [Telemetry](../telemetry.md).
- Exclude secrets and sensitive paths from edit prediction.
- Prefer local models or a controlled gateway when data must stay within your
  environment.
- Do not submit ratings or feedback until you have verified the configured
  destination and its retention policy.

See [Feedback and Training Data](./ai-improvement.md) for opt-in boundaries.
