---
title: Updating an Extension
description: "Update an extension published in Zed's upstream registry."
---

# Updating an Extension {#updating-an-extension}

> This workflow updates an entry in Zed's upstream extension registry. It does not update an Orion-owned registry entry, and upstream maintainers control the review process.

To update an extension there, open a PR to [the upstream `zed-industries/extensions` repository](https://github.com/zed-industries/extensions).

> Update PRs are subject to the same [pull request rules](./publishing-guide.md#pull-request-rules) as new submissions.

In your PR, do the following:

1. Update the extension's submodule to the commit of the new version. For this, you can run

```sh
# From the root of the repository:
git submodule update --remote extensions/your-extension-name
```

to update your extension to the latest commit available in your remote repository.

2. Update the `version` field for the extension in `extensions.toml`.
   - Make sure the `version` matches the one set in `extension.toml` at the particular commit.

If you'd like to automate this process, there is a [third-party community GitHub Action](https://github.com/huacnlee/zed-extension-action) you can use. Its repository name is retained exactly; Orion Studio does not maintain or endorse it.

For questions around maintaining (or no longer maintaining) your extension, see the [FAQ](./faq.md).
