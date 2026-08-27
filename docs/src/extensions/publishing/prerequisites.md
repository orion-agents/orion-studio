---
title: Extension Publishing Prerequisites
description: "Review requirements for publishing to Zed's upstream extension registry."
---

# Extension Publishing Prerequisites {#extension-publishing-prerequisites}

> These requirements are maintained and enforced by Zed's upstream extension registry. Orion Studio does not operate that registry or decide whether a submission is accepted.

Before submitting your extension upstream, make sure it meets the following requirements. Upstream maintainers may delay or reject a non-compliant submission.

## General Requirements

- Test your extension in the upstream Zed application at the submodule commit you are submitting. Testing it in Orion Studio is useful compatibility validation but does not replace upstream review.
- Publish functionality that is not already available in the upstream registry.
  - If you face issues with an existing extension, first try contributing to the existing extension. See the [FAQ](./faq.md#reporting-issues-and-improvements) for more details on this requirement.
- Do not misuse the extension API to work around its current limitations.
  - In rare cases, upstream maintainers may accept a reasonable workaround.
  - Acceptance of a workaround is at their discretion.
- Use an appropriate ID for your extension. The ID must:
  - be unique
  - be kebab-cased
  - not include the words `zed` or `extension`, as required by the upstream registry
  - be a good indicator for what your extension provides (more on this below)
- Include only the resources your extension needs to function.
- License your extension under one of the [allowed licenses](./license-requirements.md).
- Write all user-facing text in English.
- Do not read or modify anything outside the sandbox that the host application designates for your extension.
  - Use the [`zed_extension_api` Rust API](https://docs.rs/zed_extension_api/latest/zed_extension_api/) to read and modify the environment. The package name is retained as an extension ABI compatibility identifier.
  - Use Rust standard library methods to read and modify the work directory provided to the extension.
  - Ask the user to make any other required changes themselves.

## Language Extensions

- Only provide support for the language your extension targets, as well as any dialects that are directly associated with that language.
- Define a [grammar](../languages.md#grammar) in your extension's `extension.toml` for every language that you provide.
- You may also provide language servers, debuggers and snippets for that language.
- Choose an extension ID and name similar or equal to the name of the primary language you intend to add.
- Do not include any Rust code should your extension not also provide language servers.

## Language Server Extensions

- Should your extension only provide a language server, make sure your extension ID reflects that (e.g., by suffixing it with `-language-server` or `-lsp`).
- Do not bundle a language server with the extension. Download it or check for it in the user's environment through the [`zed_extension_api` Rust API](https://docs.rs/zed_extension_api/latest/zed_extension_api/).

## Debugger Extensions

- Should your extension only provide a debugger, make sure your extension ID reflects that (e.g., by suffixing it with `-debugger`).
- Do not bundle the debug adapter with the extension. Download it or check for it in the user's environment through the [`zed_extension_api` Rust API](https://docs.rs/zed_extension_api/latest/zed_extension_api/).

## Theme Extensions

- Only provide themes and nothing else.
- Make sure your extension ID indicates it is a theme (e.g., by suffixing it with `-theme`).

## Icon Theme Extensions

- Only provide an icon theme and nothing else.
- Make sure your extension ID indicates it is an icon theme (e.g., by suffixing it with `-icon-theme` or `-icons`).

## Snippet Extensions

- Should your extension only provide snippets, make sure your extension ID reflects that (e.g., by suffixing it with `-snippets`).
- Only scope your snippets to the global scope if appropriate. Scope language-specific snippets to the given languages.

## MCP Server Extensions

> The upstream Zed project plans to deprecate MCP server extensions in favor of the MCP registry; upstream progress is tracked in [zed-industries/zed#59351](https://github.com/zed-industries/zed/issues/59351). Publish the server to the MCP registry as well if it must remain available to future upstream Zed versions.

- Only provide one MCP server and nothing else.
- Make sure your extension ID indicates it is an MCP server (e.g., by prefixing it with `mcp-server-` or suffixing it with `-mcp-server`).
- Do not bundle the MCP server with the extension. Download it or check for it in the user's environment through the [`zed_extension_api` Rust API](https://docs.rs/zed_extension_api/latest/zed_extension_api/).

## Agent Server and Slash Command Extensions

The upstream registry no longer accepts agent server or slash command extension submissions. To make an agent server available to Orion Studio, publish it to the [ACP Registry](https://agentclientprotocol.com/registry) instead.

---

Passing everything on this list? Let's [get your extension published!](./publishing-guide.md)
