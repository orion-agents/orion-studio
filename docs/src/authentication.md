---
title: Authentication
description: Configure Orion Studio authentication only after deploying a compatible account service.
---

# Authentication

Authentication is not required for local editing, provider API keys, local
models, or many external-agent workflows.

Sign-in is deployment-dependent. It is needed only for features backed by a
configured account service, such as operator-hosted models, organization
management, or a deployed collaboration service. No public Orion sign-in URL is
assumed to exist, and Orion Studio must not fall back to Zed authentication.

## After an account service is configured

1. Select **Sign In** or run {#action client::SignIn} from the command palette.
2. Complete the authentication flow exposed by the configured operator.
3. Confirm that the browser returns to the canonical `orion://` callback.

The operator must document its identity provider, requested scopes, account
recovery, data use, session lifetime, and sign-out behavior. If the account
service is unavailable, local workflows should continue and service-backed
features should show an actionable error.

Run {#action client::SignOut} or use the profile menu to end a configured
session.

To hide sign-in UI when no service is configured, set `show_sign_in` to `false`.
See [Visual Customization](./visual-customization.md) and
[Service Availability](./orion-studio-upstream-and-compatibility.md#service-availability).
