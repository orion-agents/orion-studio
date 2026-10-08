---
title: Plans and Pricing Availability
description: Current Orion Studio plan availability and requirements for future hosted offerings.
---

# Plans and Pricing Availability

No public Orion Studio Free, Pro, Student, or Business subscription is assumed
to be available. This repository does not publish prices, included credits,
spend limits, or service-level commitments.

The open-source desktop application can use provider API keys, compatible
subscriptions, gateways, local models, and external agents according to each
provider's own terms. See [LLM Providers](../ai/llm-providers.md).

## Plans {#plans}

|                                                          | Free | Pro       | Student   | Business  |
| -------------------------------------------------------- | ---- | --------- | --------- | --------- |
| Orion-hosted AI models                                     | —    | ✓         | ✓         | ✓         |
| [AI via own API keys](../ai/use-api-access.md)           | ✓    | ✓         | ✓         | ✓         |
| [External Agents](../ai/external-agents.md)              | ✓    | ✓         | ✓         | ✓         |
| Edit Predictions                                         | —    | Unlimited | Unlimited | Unlimited |
| [Org-wide admin controls](../business/admin-controls.md) | —    | —         | —         | ✓         |
| Roles & permissions                                      | —    | —         | —         | ✓         |
| Consolidated billing                                     | —    | —         | —         | ✓         |

### Orion Studio Free {#free}

Orion Studio is free to use. You can configure AI agents with your own API keys via [Use API Access](../ai/use-api-access.md). Orion Studio's hosted models and [Edit Predictions](../ai/edit-prediction.md) require a Pro subscription. You can still use edit predictions from [other providers](../ai/edit-prediction.md#other-providers), like GitHub Copilot or a local model.

### Orion Studio Pro {#pro}

Orion Studio Pro includes access to all hosted AI models and Edit Predictions. The plan includes $5 of monthly token credit; usage beyond that is billed at the rates listed on [Orion Studio-Hosted Models](./zed-hosted-models.md). A [trial of Orion Studio Pro](#trials) includes $5 of GPT-6 Luna usage and unlimited Edit Predictions for 14 days from when you start the trial.

For details on billing and payment, see [Individual Billing](./billing.md).

### Orion Studio Business {#business}

Orion Studio Business gives members with a paid Business seat access to all of Orion Studio's hosted AI models, unlimited Edit Predictions, plus org-wide controls for administrators: which AI features are available, what data leaves your organization, and how AI spend is tracked. Paid seats and AI usage are consolidated into a single invoice.

For a full feature overview, see [Orion Studio Business](../business/overview.md). For billing details, see [Billing](./billing.md#organization).

### Student Plan {#student}

The [Zed Student plan](https://zed.dev/education) includes unlimited [Edit Predictions](../ai/edit-prediction.md), $10/month in token credits, and all [hosted AI models](./zed-hosted-models.md) except Claude Fable, Claude Opus, GPT-6 Astra, GPT-5.5 pro, and GPT-5.4 pro. The plan is available free for one year to verified university students.

## Usage {#usage}

Usage of Orion Studio's hosted models is measured on a token basis, converted to dollars at the rates listed on [Orion Studio-Hosted Models](./zed-hosted-models.md) (list price from the provider, +10%).

Monthly included credit resets on your monthly billing date. To view your current usage, navigate to the Billing page at [dashboard.zed.dev](https://dashboard.zed.dev). Usage data from our metering provider, Orb, is embedded on that page.

## Spend Limits {#usage-spend-limits}

### Orion Studio Pro {#pro-spend-limits}

On your Billing page you'll find an input for `Monthly Spend Limit`. For Orion Studio Pro, the dollar amount here specifies your pre-tax _monthly_ limit for spend on tokens, _not counting_ the $5/month included with your Pro subscription.

The default value for Pro users is $10, for a total monthly spend with Orion Studio of $20 ($10 for your Pro subscription, $10 in incremental token spend). This can be set to $0 to limit your spend with Orion Studio to exactly $10/month. If you adjust this limit _higher_ than $10 and consume more than $10 of incremental token spend, that usage may be billed during the month via [Orion Studio Pro threshold billing](./billing.md#threshold-billing).

Once the spend limit is hit, we'll stop any further usage until your token spend limit resets.

### Orion Studio Business {#business-spend-limits}

On Orion Studio Business, administrators set a pre-tax org-wide spend limit from the Data & Privacy page in the organization dashboard. Seats and AI usage are consolidated into [Organization billing](./billing.md#organization). Once the org-wide spend limit is reached, we'll stop hosted model usage for members until the limit resets or an administrator raises it.

> **Note:** Spend limits are a Orion Studio Pro and Business feature. Student plan users cannot configure spend limits; usage is capped at the $10/month included credit.

### Trials {#trials}

Trials include $5 of GPT-6 Luna usage and unlimited Edit Predictions for 14 days from when you start the trial. GPT-6 Luna is the only hosted model available during the trial. Orion Studio and Delta share the same $5 trial balance. No credit card is required.

Trials automatically convert to Orion Studio Free when they end. No cancellation is needed to prevent conversion to Orion Studio Pro.
