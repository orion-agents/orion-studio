---
title: Feedback and Training Data
description: Verify Orion Studio feedback and training destinations before opting in to data sharing.
---

# Feedback and Training Data

Orion Studio does not assume that a public feedback or training-data service is
deployed. Client telemetry is disabled by default, and an operator must
configure and disclose any destination before collecting prompts, code,
ratings, or edit-prediction samples.

## Response ratings and feedback

A rating or feedback action can include your messages, model responses, thread
metadata, and installation metadata. Use it only after verifying the configured
endpoint, retention period, access controls, deletion process, and privacy
policy. A single opt-in must not authorize unrelated future collection.

## Edit prediction training

If a deployment enables training-data collection, require all of the following:

1. explicit user opt-in;
2. an operator-owned and documented endpoint;
3. exclusion of secrets and sensitive paths;
4. clear retention, review, anonymization, and deletion rules;
5. a visible way to disable collection again.

Add local exclusions with `edit_predictions.disabled_globs`:

```json [settings]
{
  "edit_predictions": {
    "disabled_globs": [
      "**/.env*",
      "**/*.pem",
      "**/*.key",
      "**/secrets/**",
      "private/**"
    ]
  }
}
```

The Zeta datasets and models under the `zed-industries` Hugging Face namespace
are upstream Zed artifacts. Their names and terms are retained as upstream
attribution; they are not evidence of an Orion-hosted data pipeline.

For request-path boundaries, see [AI Privacy and Security](./privacy-and-security.md).
