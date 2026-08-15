---
title: Orion-Hosted Models
description: Configure Orion-hosted models only after an operator deploys the required account, model, and billing services.
---

# Orion-Hosted Models

The filename of this page retains `zed-hosted-models` so historical links keep
working. The product-facing feature name is **Orion-hosted models**.

Orion-hosted models are deployment-dependent. The canonical provider setting is
`orion.dev`, but that identifier does not prove that a public endpoint exists.
An operator must deploy and configure authentication, model routing, metering,
billing, privacy controls, retention policy, abuse controls, and support before
this provider can be used.

Until then, configure one of the available non-Orion paths:

- [Provider API access](../ai/use-api-access.md)
- [An existing provider subscription](../ai/use-an-existing-subscription.md)
- [A gateway](../ai/use-a-gateway.md)
- [A local or self-hosted model](../ai/use-a-local-model.md)
- [An External Agent](../ai/external-agents.md)

Legacy settings may still contain the provider ID `zed.dev`. Orion Studio reads
that value only for migration compatibility; new configuration uses
`orion.dev`. Neither value should silently route traffic to a Zed-hosted
service.
