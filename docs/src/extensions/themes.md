---
title: Themes
description: "Themes for Orion Studio extensions."
---

# Themes

The `themes` directory in an extension should contain one or more theme files.

Each theme file must match the schema implemented by Orion Studio's [`theme` crate](https://github.com/orion-agents/orion-studio/tree/main/crates/theme/src). A hosted `orion.dev` schema endpoint is not assumed to be deployed.

## Theme JSON Structure

The structure of an Orion Studio theme is defined by the theme types in the repository.

An Orion Studio theme consists of a Theme Family object including:

- `name`: The name for the theme family
- `author`: The name of the author of the theme family
- `themes`: An array of Themes belonging to the theme family

The core components of a Theme object include:

1. Theme Metadata:

   - `name`: The name of the theme
   - `appearance`: Either "light" or "dark"

2. Style Properties under the `style`, such as:

   - `background`: The main background color
   - `foreground`: The main text color
   - `accent`: The accent color used for highlighting and emphasis

3. Syntax Highlighting:

   - `syntax`: An object containing color definitions for various syntax elements (e.g., keywords, strings, comments)

4. UI Elements:

   - Colors for various UI components such as:
     - `element.background`: Background color for UI elements
     - `border`: Border colors for different states (normal, focused, selected)
     - `text`: Text colors for different states (normal, muted, accent)

5. Editor-specific Colors:

   - Colors for editor-related elements such as:
     - `editor.background`: Editor background color
     - `editor.gutter`: Gutter colors
     - `editor.line_number`: Line number colors

6. Terminal Colors:
   - ANSI color definitions for the integrated terminal

## Designing Your Theme

Start from an existing theme in [`assets/themes`](https://github.com/orion-agents/orion-studio/tree/main/assets/themes), edit the JSON, and test it as a local or development extension. A hosted Orion Theme Builder or public extension store is not assumed to be available.
