---
title: Publishing Guide
description: "Submit compatible extensions to Zed's upstream extension registry."
---

# Publishing Guide {#publishing-your-extension}

> This guide targets Zed's upstream extension registry at `zed-industries/extensions`. It does not publish to an Orion-owned registry, and Zed's maintainers review these pull requests independently of Orion Studio.

Before initiating the upstream publishing process, read and ensure that your extension meets all [publishing prerequisites](./prerequisites.md) and [license requirements](./license-requirements.md). The upstream registry may delay or reject submissions that do not satisfy these requirements.

Follow each step carefully to help the publishing process go smoothly.

## Pull request rules {#pull-request-rules}

To keep its review queue manageable, the upstream Zed project applies the following rules to every PR against the `zed-industries/extensions` repository:

- Every PR must add or update **exactly one extension**.
- You may have **at most three open PRs** at any given time.
- Respond to maintainer feedback within **3 weeks**, otherwise your PR will be closed.

PRs that do not adhere to these rules will be closed without further feedback. Repeated violations of these rules may result in a temporary suspension or a ban from submitting to the extension repository.

## Forking and cloning the repo

1. Fork the upstream `zed-industries/extensions` repository.

> **Note:** The upstream maintainers recommend forking `zed-industries/extensions` to a personal GitHub account instead of an organization. This can let Zed staff push needed changes to that upstream PR; it does not grant them an Orion Studio support role.

2. Clone the repo to your local machine.

```sh
# Substitute the URL of your fork here:
git clone https://github.com/your-username/extensions
cd extensions
git submodule init
git submodule update
```

## Submitting your extension {#submitting-your-extension}

To publish through the upstream Zed registry, open a PR to [the `zed-industries/extensions` repository](https://github.com/zed-industries/extensions).

In your PR, do the following:

1. Add your extension as a Git submodule within the `extensions/` directory under the `extensions/{extension-id}` path.
   - The submodule must use an HTTPS URL and not an SSH URL (`git@github.com`).
   - Your extension repository must be publicly available.
   - The checked out submodule commit must be present on a branch and thus not be a detached commit.

```sh
git submodule add https://github.com/your-username/foobar.git extensions/my-extension
git add extensions/my-extension
```

2. Add a new entry to the top-level `extensions.toml` file containing your extension:
   - Make sure the `version` matches the one set in `extension.toml` at the particular commit.

```toml
[my-extension]
submodule = "extensions/my-extension"
version = "0.0.1"
```

If your extension is in a subdirectory within the submodule, you can use the `path` field to point to where the extension resides:

```toml
[my-extension]
submodule = "extensions/my-extension"
path = "packages/editor-extension"
version = "0.0.1"
```

3. Run `pnpm sort-extensions` to ensure `extensions.toml` and `.gitmodules` are sorted.

Once the upstream PR is accepted and merged, the extension will be packaged and published to Zed's upstream extension registry.

## Review process {#review-process}

Once your PR is open, an upstream maintainer will review it and may request changes. Keep the [pull request rules](#pull-request-rules) in mind: the upstream project closes submissions after **3 weeks of no response to maintainer feedback**.

For everything beyond that - how long upstream reviews take, why that project enforces the timeframe, or what to do after a PR was closed - see the [FAQ](./faq.md).
