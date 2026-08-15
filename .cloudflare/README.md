Two Cloudflare Workers serve Orion Studio documentation and website assets:

- `open-source-website-assets` is used for `install.sh`
- `docs-proxy` is used for `https://orion.dev/docs`

The checked-in `wrangler.toml` files are deliberately non-routable: `workers_dev` is disabled and no production route or R2 bucket is bound. Both Workers return HTTP 503 when required `ORION_STUDIO_*` runtime bindings are absent or invalid. This prevents an incomplete deployment from falling back to upstream infrastructure.

### Deployment

Generate a deployment-only Wrangler configuration after the Orion Cloudflare resources exist. The renderer exits nonzero when any required setting is absent, a route differs from the approved `orion.dev/docs*` or `orion.dev/install.sh` boundary, a docs origin is not an `orion-studio-*.pages.dev` project, the website origin is not an approved Orion host, or a bucket lacks the `orion-studio-` prefix.

Required for both Workers:

- `ORION_STUDIO_CLOUDFLARE_ZONE_NAME=orion.dev`
- `ORION_STUDIO_WEBSITE_ORIGIN`

Required for `docs-proxy`:

- `ORION_STUDIO_DOCS_ROUTE_PATTERN`
- `ORION_STUDIO_DOCS_STABLE_ORIGIN`
- `ORION_STUDIO_DOCS_PREVIEW_ORIGIN`
- `ORION_STUDIO_DOCS_NIGHTLY_ORIGIN`

Required for `open-source-website-assets`:

- `ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_ROUTE_PATTERN`
- `ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME`

Example generation commands:

```sh
node .cloudflare/render-wrangler-config.mjs docs-proxy /tmp/orion-studio-docs-proxy.toml
node .cloudflare/render-wrangler-config.mjs open-source-website-assets /tmp/orion-studio-assets.toml
```

Review the generated file, then pass it explicitly to Wrangler with `--config`. Do not deploy either base `wrangler.toml` directly. The docs deployment workflow must be updated to call this renderer and to use Orion-owned Cloudflare Pages projects, route, R2 bucket, account, and credentials before production deployment is enabled.

### Testing

Use [Wrangler](https://developers.cloudflare.com/workers/wrangler/) to test a generated configuration locally. A missing binding should produce HTTP 503; it must never proxy to a legacy service.
