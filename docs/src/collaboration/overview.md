---
title: Collaboration
description: Configure Orion Studio real-time collaboration with an operator-deployed service.
---

# Collaboration {#collaboration}

Orion Studio contains real-time collaboration client and server code, but no
public Orion collaboration or authentication service is assumed to be deployed.
These features become available only after an operator configures compatible
account, collaboration, database, WebSocket, and LiveKit services.

The client must not silently route collaboration traffic to Zed-hosted
infrastructure. When no service is configured, local editing and other offline
workflows remain available.

After deployment, open the Collaboration Panel with
{#kb collab_panel::ToggleFocus}. A configured sign-in flow may then expose
[Channels](./channels.md) and
[Contacts and Private Calls](./contacts-and-private-calls.md).

> **Security:** sharing a project grants collaborators access to files within
> that project and may expose terminals, language servers, and screen content.
> Collaborate only with trusted people and review the operator's privacy and
> retention policy.

Audio device settings are available under **Collaboration > Experimental** in
Settings ({#kb zed::OpenSettings}) after the feature is enabled.
