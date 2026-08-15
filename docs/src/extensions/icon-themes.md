---
title: Icon Themes
description: "Icon Themes for Orion Studio extensions."
---

# Icon Themes

Extensions may provide icon themes to change the icons Orion Studio uses for folders and files.

## Example extension

The upstream-Zed [Material Icon Theme](https://github.com/zed-extensions/material-icon-theme) serves as a compatibility example for the structure of an extension containing an icon theme.

## Directory structure

There are two important directories for an icon theme extension:

- `icon_themes`: This directory will contain one or more JSON files containing the icon theme definitions.
- `icons`: This directory contains the icon assets distributed with the extension. You can create subdirectories in this directory as needed.

Each icon theme file must match the schema implemented in [`crates/theme/src/icon_theme_schema.rs`](https://github.com/orion-agents/orion-studio/blob/main/crates/theme/src/icon_theme_schema.rs). A hosted `orion.dev` schema endpoint is not assumed to be deployed.

Here is an example icon theme structure:

```json [icon-theme]
{
  "name": "My Icon Theme",
  "author": "Your Name",
  "themes": [
    {
      "name": "My Icon Theme",
      "appearance": "dark",
      "directory_icons": {
        "collapsed": "./icons/folder.svg",
        "expanded": "./icons/folder-open.svg"
      },
      "named_directory_icons": {
        "stylesheets": {
          "collapsed": "./icons/folder-stylesheets.svg",
          "expanded": "./icons/folder-stylesheets-open.svg"
        }
      },
      "chevron_icons": {
        "collapsed": "./icons/chevron-right.svg",
        "expanded": "./icons/chevron-down.svg"
      },
      "file_stems": {
        "Makefile": "make"
      },
      "file_suffixes": {
        "mp3": "audio",
        "rs": "rust"
      },
      "file_icons": {
        "audio": { "path": "./icons/audio.svg" },
        "default": { "path": "./icons/file.svg" },
        "make": { "path": "./icons/make.svg" },
        "rust": { "path": "./icons/rust.svg" }
        // ...
      }
    }
  ]
}
```

Each icon path is resolved relative to the root of the extension directory.

In this example, the extension would have this structure:

```text
extension.toml
icon_themes/
  my-icon-theme.json
icons/
  audio.svg
  chevron-down.svg
  chevron-right.svg
  file.svg
  folder-open.svg
  folder.svg
  rust.svg
```
