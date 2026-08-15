# Orion Studio Extensions

This directory contains extensions for Orion Studio that are largely maintained by the Orion Studio team. They currently live in the Orion Studio repository for ease of maintenance.

The Orion extension registry is deployment-dependent and is not enabled until an operator configures an Orion-owned registry. Existing extensions may still use upstream Zed identifiers where the extension ABI requires compatibility; see [Upstream Attribution and Compatibility](../docs/src/orion-studio-upstream-and-compatibility.md).

## Structure

Orion Studio includes support for a number of languages without requiring an extension. Those languages live under [`crates/languages/src`](../crates/languages/src).

Support for other languages is provided by extensions. This directory contains the extensions maintained with Orion Studio. They currently use the compatibility-named [`zed_extension_api`](https://docs.rs/zed_extension_api/latest/zed_extension_api/) ABI for language servers, tree-sitter grammars, queries, themes, and related integrations.

## Dev Extensions

See [Developing an Extension Locally](../docs/src/extensions/developing-extensions.md#developing-an-extension-locally) for how to work with one of these extensions.
