---
title: Frequently Asked Questions
description: "Summarize policies for publishing and maintaining extensions in Zed's upstream registry."
---

# Frequently Asked Questions {#faq}

> This page summarizes policies for Zed's upstream extension registry. Orion Studio does not operate that registry, control its review queue, or provide support on behalf of Zed staff.

Questions come up before, during, and after publishing an extension to the upstream registry. These are the questions addressed by its current policies.

## How long will the review of my submission take? {#review-duration}

Zed's upstream maintainers report that most submissions receive initial feedback within a few weeks, while some take one or two months. These estimates are not a service commitment from Orion Studio, and the upstream review can take longer when its backlog is large.

## Why was my PR closed? {#pr-closed}

If an upstream PR is closed without much feedback, it may have substantially violated the [publishing prerequisites](./prerequisites.md). The upstream maintainers may not provide additional context when submission volume is high.

As stated in the [pull request rules](./publishing-guide.md#pull-request-rules), the upstream registry considers submissions stale after **3 weeks of no response to maintainer feedback** and may close them.

A PR closed due to staleness can be resubmitted as a fresh upstream PR.

## Why does the upstream registry enforce a contributor response timeframe? {#response-timeframe}

The upstream policy uses the timeframe to close stale submissions and keep its review queue manageable. Orion Studio does not set or enforce that policy.

## Why was I asked to open a new PR instead of continuing my closed one? {#fresh-prs}

When its backlog is high, the upstream registry may request a fresh PR to keep the queue shorter. Its maintainers have found that a fresh PR is often easier to review than a previously closed PR with a long update history.

A fresh PR can therefore be quicker for the upstream maintainers to review and merge.

## Why does the upstream registry enforce strict prerequisites? {#why-prerequisites}

The upstream prerequisites balance an open extension ecosystem with a baseline level of quality, so users do not have to vet every extension from scratch.

They also consolidate effort. Multiple near-identical extensions split users and maintenance work, while one extension per use case gives owners and contributors a shared place to improve it.

The rules do not catch every problem, but they establish the baseline used by the upstream registry and its maintenance policies.

## Do the prerequisites also apply to already published extensions? {#prerequisites-for-existing-extensions}

Yes. Existing extensions are not exempt from the prerequisites, and updates are held to the same standards as new submissions.

The only exception are the ID restrictions: since an extension's ID cannot change once published, existing IDs stay as they are.

## Do I have to maintain my extension? {#do-i-have-to-maintain-my-extension}

No ongoing maintenance is required by the upstream policy after an extension is published there.

That said, no extension is perfect on day one - bugs may surface and improvement requests may come in. While maintenance is never required, it helps everyone if you respond to those reports within a reasonable timeframe.

## I found a bug in an extension or want to improve it. What should I do? {#reporting-issues-and-improvements}

Report the issue or propose your improvement in the original extension repository first, rather than publishing a competing extension. Most owners appreciate reports and contributions, and keeping the effort in one place spares users from having to pick between near-identical extensions - as well as maintainers from having to review them.

If the owner does not respond for an extended period of time, see [What happens when an extension owner stops responding?](#unresponsive-owner)

## I no longer want to maintain my extension. What now? {#stepping-back}

Completely fine - priorities change, and stepping back from an extension is nothing to feel bad about. As the current owner, you can:

- transfer ownership of the repository to a new owner, or
- open an issue or pull request against the upstream [`zed-industries/extensions`](https://github.com/zed-industries/extensions) repository asking for the removal of your extension.

Note that you may also just keep your extension around - many extensions have received few to no updates and yet have many happy users.

## What happens when an extension owner stops responding to reported issues? {#unresponsive-owner}

Under Zed's upstream policy, if an extension owner can no longer be reached:

- a contributor may fork the extension and propose their fork as a replacement for the current extension, or
- Zed staff may fork the extension into the upstream [`zed-extensions`](https://github.com/zed-extensions) organization, where maintenance continues as a joint effort of that community and Zed staff.

The upstream policy permits either action only if one of the following requirements is met:

- the current owner has given written permission, or
- there is **written proof** of attempts to establish contact, and the owner has been unresponsive to them for **at least 6 weeks**.

Without one of these, the extension stays with its current owner as is.

Such a switch also does not have to be permanent: should the original owner become responsive again, the extension may be switched back to the original repository.

## Why can't my language reuse builtin grammars or a grammar from another extension? {#grammar-reuse}

If your language depended on a grammar it does not own, updates to that grammar could change the nodes produced by Tree-sitter parsing and silently break your language. To avoid this, every language must use a grammar defined in its own extension's `extension.toml`.

## I have questions about these policies or disagree with them. Where can I raise that? {#policy-discussion}

Questions about these upstream policies belong in a discussion in the upstream [`zed-industries/zed`](https://github.com/zed-industries/zed) repository. That repository is a Zed support and policy channel, not an Orion Studio support channel.
