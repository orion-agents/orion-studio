use client::{ORION_URL_SCHEME, ZED_URL_SCHEME};
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the orion:// and zed:// URL scheme handlers.
        RegisterZedScheme
    ]
);

pub async fn register_zed_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    // Register the canonical `orion://` scheme, then the legacy `zed://`
    // scheme so existing links and bookmarks keep working during the
    // migration window (S02 compatibility contract).
    cx.update(|cx| cx.register_url_scheme(ORION_URL_SCHEME))
        .await?;
    cx.update(|cx| cx.register_url_scheme(ZED_URL_SCHEME)).await
}
