#!/usr/bin/env node

import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const worker = process.argv[2];
const outputPath = process.argv[3];

if (!worker || !outputPath || !["docs-proxy", "open-source-website-assets"].includes(worker)) {
  fail("Usage: node .cloudflare/render-wrangler-config.mjs <docs-proxy|open-source-website-assets> <output-path>");
}

const zoneName = required("ORION_STUDIO_CLOUDFLARE_ZONE_NAME");
if (zoneName !== "orion.dev") {
  fail("ORION_STUDIO_CLOUDFLARE_ZONE_NAME must be the approved Orion zone: orion.dev");
}

const lines = [
  `name = ${tomlString(`orion-studio-${worker}`)}`,
  `main = ${tomlString(workerEntrypoint(worker))}`,
  'compatibility_date = "2024-05-15"',
  "workers_dev = false",
  "",
  "[[routes]]",
  `pattern = ${tomlString(routePattern(worker))}`,
  `zone_name = ${tomlString(zoneName)}`,
];

if (worker === "docs-proxy") {
  const docsOrigins = [
    requiredPagesOrigin("ORION_STUDIO_DOCS_STABLE_ORIGIN"),
    requiredPagesOrigin("ORION_STUDIO_DOCS_PREVIEW_ORIGIN"),
    requiredPagesOrigin("ORION_STUDIO_DOCS_NIGHTLY_ORIGIN"),
  ];
  if (new Set(docsOrigins).size !== docsOrigins.length) {
    fail("Stable, preview, and nightly docs origins must be distinct Orion Studio Pages origins");
  }
  lines.push(
    "",
    "[vars]",
    `ORION_STUDIO_DOCS_STABLE_ORIGIN = ${tomlString(docsOrigins[0])}`,
    `ORION_STUDIO_DOCS_PREVIEW_ORIGIN = ${tomlString(docsOrigins[1])}`,
    `ORION_STUDIO_DOCS_NIGHTLY_ORIGIN = ${tomlString(docsOrigins[2])}`,
    `ORION_STUDIO_WEBSITE_ORIGIN = ${tomlString(requiredWebsiteOrigin())}`,
  );
} else {
  lines.push(
    "",
    "[vars]",
    `ORION_STUDIO_WEBSITE_ORIGIN = ${tomlString(requiredWebsiteOrigin())}`,
    "",
    "[[r2_buckets]]",
    'binding = "ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET"',
    `bucket_name = ${tomlString(requiredResourceName("ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET_NAME"))}`,
  );
}

try {
  await writeFile(outputPath, `${lines.join("\n")}\n`, { flag: "wx" });
} catch (error) {
  fail(`Unable to create ${outputPath}: ${error.message}`);
}

console.log(outputPath);

function workerEntrypoint(workerName) {
  return fileURLToPath(new URL(`./${workerName}/src/worker.js`, import.meta.url));
}

function routePattern(workerName) {
  const name =
    workerName === "docs-proxy"
      ? "ORION_STUDIO_DOCS_ROUTE_PATTERN"
      : "ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_ROUTE_PATTERN";
  const value = required(name);
  const approvedPattern = workerName === "docs-proxy" ? "orion.dev/docs*" : "orion.dev/install.sh";
  if (value !== approvedPattern) {
    fail(`${name} must exactly match the approved route: ${approvedPattern}`);
  }
  return value;
}

function requiredPagesOrigin(name) {
  const value = required(name);
  let url;
  try {
    url = new URL(value);
  } catch {
    fail(`${name} must be a valid HTTPS origin`);
  }

  if (
    url.protocol !== "https:" ||
    url.username !== "" ||
    url.password !== "" ||
    url.pathname !== "/" ||
    url.search !== "" ||
    url.hash !== "" ||
    !/^orion-studio-[a-z0-9]+(?:-[a-z0-9]+)*\.pages\.dev$/.test(url.hostname)
  ) {
    fail(`${name} must be a credential-free Orion Studio pages.dev origin`);
  }
  return url.origin;
}

function requiredWebsiteOrigin() {
  const name = "ORION_STUDIO_WEBSITE_ORIGIN";
  const value = required(name);
  let url;
  try {
    url = new URL(value);
  } catch {
    fail(`${name} must be the approved Orion Studio website origin`);
  }

  if (
    url.protocol !== "https:" ||
    url.username !== "" ||
    url.password !== "" ||
    url.pathname !== "/" ||
    url.search !== "" ||
    url.hash !== "" ||
    !["orion.dev", "www.orion.dev"].includes(url.hostname)
  ) {
    fail(`${name} must be https://orion.dev or https://www.orion.dev`);
  }
  return url.origin;
}

function requiredResourceName(name) {
  const value = required(name);
  if (!/^orion-studio-[a-z0-9-]+$/.test(value)) {
    fail(`${name} must use an orion-studio- prefixed Cloudflare resource name`);
  }
  return value;
}

function required(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    fail(`Missing required environment variable: ${name}`);
  }
  return value;
}

function tomlString(value) {
  return JSON.stringify(value);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
