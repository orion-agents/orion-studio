---
title: Icon Themes
description: "Orion Studio comes with a built-in icon theme, with more icon themes available as extensions."
---

# Icon Themes

Orion Studio comes with a built-in icon theme, with more icon themes available as extensions.

## Selecting an Icon Theme

See what icon themes are installed and preview them via the Icon Theme Selector, which you can open from the command palette with {#action icon_theme_selector::Toggle}.

Navigating through the icon theme list by moving up and down will change the icon theme in real time and hitting enter will save it to your settings file.

## Installing more Icon Themes

Browse available icon-theme extensions from the in-app Extensions page with {#action zed::Extensions}. A public Orion extension catalog is not assumed to be deployed.

## Configuring Icon Themes

Your selected icon theme is stored in your settings file.
You can open your settings file from the command palette with {#action zed::OpenSettingsFile} (bound to {#kb zed::OpenSettingsFile}).

Just like with themes, Orion Studio allows for configuring different icon themes for light and dark mode.
You can set the mode to `"light"` or `"dark"` to ignore the current system mode.

```json [settings]
{
  "icon_theme": {
    "mode": "system",
    "light": "Light Icon Theme",
    "dark": "Dark Icon Theme"
  }
}
```

## Icon Theme Development

See: [Developing Orion Studio Icon Themes](./extensions/icon-themes.md)
