---
title: Billing Service Availability
description: Billing requirements for an operator-deployed Orion Studio account service.
---

# Billing Service Availability

Orion Studio does not currently assume that a public billing service,
subscription dashboard, or paid plan is deployed. The desktop application can
be built and used with local models, provider API keys, gateways, and external
agents without an Orion billing account.

If an operator deploys Orion-hosted models or collaboration and charges for
them, that deployment must publish its own pricing, billing portal, invoice
policy, tax handling, support contact, data policy, and cancellation terms. Do
not use Orion Studio billing pages, accounts, or support addresses for Orion Studio.

## Individual billing {#individual}

### Billing information {#settings}

Access billing information and settings from your [Zed dashboard](https://dashboard.zed.dev).
This page embeds data from Orb, our invoicing and metering partner.

### Billing cycles {#billing-cycles}

Orion Studio is billed on a monthly basis based on the date you initially subscribe. You'll receive _at least_ one invoice from Orion Studio each month you're subscribed to Orion Studio Pro, and may receive more than one invoice if you use [hosted models](./zed-hosted-models.md) beyond your included monthly token credit.

### Orion Studio Pro threshold billing {#threshold-billing}

For individual Orion Studio Pro subscriptions, Orion Studio uses threshold billing to ensure timely payment collection. Threshold billing controls when already-allowed token usage is invoiced during the month; your [monthly spend limit](./plans-and-pricing.md#usage-spend-limits) still controls when hosted model usage stops.

Threshold invoices start at $10 of pre-tax incremental token spend. For higher token usage, Orion Studio may automatically raise your pre-tax invoicing threshold in $10 increments, up to $100, so you receive fewer mid-cycle invoices. Once raised, the invoicing threshold is not automatically lowered during the same subscription.

For Orion Studio Business billing, see [Organization billing](#organization).

For example,

- You subscribe on February 1. Your first invoice is $10.
- You use $12 of incremental tokens in the month of February, with the first $10 spent on February 15. You'll receive an invoice for $10 on February 15.
- On March 1, you receive your next monthly subscription invoice, plus any remaining token spend that was not already invoiced during February.

### Payment failures {#payment-failures}

If payment of an invoice fails, Zed will block usage of our hosted models until the payment is complete. Email [billing-support@zed.dev](mailto:billing-support@zed.dev) for assistance.

### Invoice history {#invoice-history}

You can access your invoice history from the Billing page at [dashboard.zed.dev](https://dashboard.zed.dev) by clicking `Invoice history` within the embedded Orb portal.

If you require historical Stripe invoices, email [billing-support@zed.dev](mailto:billing-support@zed.dev).

## Organization billing {#organization}

Orion Studio Business consolidates your team's costs. Paid Business seats and member AI usage appear on one bill, with no separate invoices per member. For a full feature overview, see [Orion Studio Business](../business/overview.md).

### Billing dashboard {#dashboard}

Owners, admins, and Billing Managers can access billing information at [dashboard.zed.dev](https://dashboard.zed.dev). The dashboard shows the current plan and links to update billing details, tax ID information, and payment information. You can also access invoice history through the Orb billing portal.

Use the Billing Manager role for someone who needs billing access but does not need a paid Business seat. Billing Managers can view subscription usage, update billing details and payment methods, and access invoice history. They cannot manage members, change organization settings, cancel the subscription, or use Orion-hosted AI models and Edit Predictions through the Business subscription. For the full permissions list, see [Roles](../roles.md#role-billing-manager).

### AI usage {#ai-usage}

AI usage across the organization is metered on a token basis at the same rates as individual Pro subscriptions. See [Plans & Pricing](./plans-and-pricing.md#usage) for rate details.

Administrators can set an org-wide AI spend limit from the Data & Privacy page in the organization dashboard. The limit starts at $0, so it must be increased before members can use any hosted models. Once the limit is reached, members will see an error when attempting to use hosted models.

### Invoice history {#org-invoice-history}

Owners, admins, and Billing Managers can access an organization's invoice history from the Billing page at [dashboard.zed.dev](https://dashboard.zed.dev) by clicking `Invoice history` within the embedded Orb portal.

If you require historical Stripe invoices, email [billing-support@zed.dev](mailto:billing-support@zed.dev).

## Updating billing information {#updating-billing-info}

From the _Billing_ page, owners, admins, and Billing Managers can update billing name, address, tax ID information, and payment method.

Changes to billing information will **only** affect future invoices. We cannot modify historical invoices. Email [billing-support@zed.dev](mailto:billing-support@zed.dev) with any questions.

## Sales tax {#sales-tax}

Orion Studio partners with [Sphere](https://www.getsphere.com/) to calculate indirect tax rates for invoices, based on customer location and the product being sold. Tax is listed as a separate line item on invoices, based preferentially on your billing address, followed by the card issue country known to Stripe.

If you have a tax ID, such as a VAT or GST ID, you can add it during checkout or update it later from the Billing page. Check the box that denotes you as a business.

Changes to tax IDs and addresses will **only** affect future invoices. We cannot modify historical invoices.

Email [billing-support@zed.dev](mailto:billing-support@zed.dev) with any tax questions.
