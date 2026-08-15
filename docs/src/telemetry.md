---
title: Telemetry
description: Orion Studio telemetry defaults, controls, and deployment boundaries.
---

# Telemetry in Orion Studio

Client diagnostics and metrics are disabled by default. No public Orion
telemetry or crash-reporting endpoint is assumed to be deployed.

Open Settings with {#kb zed::OpenSettings} and search for **telemetry**, or edit
your settings file:

```json [settings]
{
  "telemetry": {
    "diagnostics": false,
    "metrics": false
  }
}
```

`zed::OpenSettings` is an internal action identifier retained for compatibility;
it does not identify the product shown in the UI.

## Enabling telemetry in a deployment

An operator must configure an Orion-owned endpoint and publish the data fields,
purpose, retention, subprocessors, regional handling, deletion process, and
contact information before enabling telemetry. The application must not send
Orion Studio diagnostics to a Zed endpoint.

When diagnostics are enabled and a compatible endpoint is configured, crash
reports may include a minidump and debug metadata. Metrics may include feature
usage and project statistics, but should not include source code or secrets.
Review the implementation and deployment policy rather than relying on this
summary alone.

Use {#action zed::OpenTelemetryLog} to inspect locally recorded telemetry
events. Source-level event definitions live in
[`crates/telemetry_events`](https://github.com/orion-agents/orion-studio/tree/main/crates/telemetry_events).

Service-side metering for hosted models or collaboration is separate from
client telemetry and applies only after those services are deployed. See
[AI Privacy and Security](./ai/privacy-and-security.md).
