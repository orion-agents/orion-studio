export default {
  async fetch(request, env, _ctx) {
    if (!isSafeMethod(request.method)) {
      return methodNotAllowed();
    }

    const configuration = docsConfiguration(env);
    if (!configuration) {
      return configurationError("Orion Studio docs proxy configuration is incomplete.");
    }

    const requestUrl = new URL(request.url);
    const acceptHeader = request.headers.get("Accept") || "";
    const wantsMarkdown = acceptHeader
      .split(",")
      .map((mediaType) => mediaType.split(";")[0].trim().toLowerCase())
      .includes("text/markdown");

    let upstreamOrigin;
    if (requestUrl.pathname === "/docs/nightly") {
      upstreamOrigin = configuration.nightlyOrigin;
      requestUrl.pathname = "/docs/";
    } else if (requestUrl.pathname.startsWith("/docs/nightly/")) {
      upstreamOrigin = configuration.nightlyOrigin;
      requestUrl.pathname = requestUrl.pathname.replace("/docs/nightly/", "/docs/");
    } else if (requestUrl.pathname === "/docs/preview") {
      upstreamOrigin = configuration.previewOrigin;
      requestUrl.pathname = "/docs/";
    } else if (requestUrl.pathname.startsWith("/docs/preview/")) {
      upstreamOrigin = configuration.previewOrigin;
      requestUrl.pathname = requestUrl.pathname.replace("/docs/preview/", "/docs/");
    } else {
      upstreamOrigin = configuration.stableOrigin;
    }

    if (requestUrl.pathname === "/docs.md") {
      requestUrl.pathname = "/docs/getting-started.md";
    }

    if (wantsMarkdown) {
      requestUrl.pathname = markdownPathFor(requestUrl.pathname);
    }

    const upstreamUrl = new URL(upstreamOrigin);
    upstreamUrl.pathname = requestUrl.pathname;
    upstreamUrl.search = requestUrl.search;
    let response = await fetch(safeUpstreamRequest(request, upstreamUrl));

    if (response.status === 404) {
      const fallbackUrl = new URL(configuration.websiteOrigin);
      fallbackUrl.pathname = "/404";
      response = await fetch(safeUpstreamRequest(request, fallbackUrl));
    }

    return response;
  },
};

function docsConfiguration(env) {
  const stableOrigin = configuredPagesOrigin(env, "ORION_STUDIO_DOCS_STABLE_ORIGIN");
  const previewOrigin = configuredPagesOrigin(env, "ORION_STUDIO_DOCS_PREVIEW_ORIGIN");
  const nightlyOrigin = configuredPagesOrigin(env, "ORION_STUDIO_DOCS_NIGHTLY_ORIGIN");
  const websiteOrigin = configuredWebsiteOrigin(env);

  if (!stableOrigin || !previewOrigin || !nightlyOrigin || !websiteOrigin) {
    return null;
  }

  return { stableOrigin, previewOrigin, nightlyOrigin, websiteOrigin };
}

function configuredPagesOrigin(env, name) {
  const value = env?.[name];
  if (typeof value !== "string" || value.trim() === "") {
    return null;
  }

  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username !== "" ||
      url.password !== "" ||
      url.pathname !== "/" ||
      url.search !== "" ||
      url.hash !== "" ||
      !/^orion-studio-[a-z0-9]+(?:-[a-z0-9]+)*\.pages\.dev$/.test(url.hostname)
    ) {
      return null;
    }
    return url;
  } catch {
    return null;
  }
}

function configuredWebsiteOrigin(env) {
  const value = env?.ORION_STUDIO_WEBSITE_ORIGIN;
  if (typeof value !== "string" || value.trim() === "") {
    return null;
  }

  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username !== "" ||
      url.password !== "" ||
      url.pathname !== "/" ||
      url.search !== "" ||
      url.hash !== "" ||
      !["orion.dev", "www.orion.dev"].includes(url.hostname)
    ) {
      return null;
    }
    return url;
  } catch {
    return null;
  }
}

function safeUpstreamRequest(request, url) {
  const headers = new Headers();
  for (const name of ["accept", "accept-encoding", "if-modified-since", "if-none-match", "range"]) {
    const value = request.headers.get(name);
    if (value !== null) {
      headers.set(name, value);
    }
  }

  return new Request(url, {
    method: request.method,
    headers,
    redirect: "manual",
  });
}

function isSafeMethod(method) {
  return method === "GET" || method === "HEAD";
}

function methodNotAllowed() {
  return new Response("Method Not Allowed", {
    status: 405,
    headers: {
      allow: "GET, HEAD",
      "cache-control": "no-store",
      "content-type": "text/plain; charset=utf-8",
    },
  });
}

function configurationError(message) {
  return new Response(message, {
    status: 503,
    headers: {
      "cache-control": "no-store",
      "content-type": "text/plain; charset=utf-8",
    },
  });
}

function markdownPathFor(pathname) {
  if (pathname === "/docs" || pathname === "/docs/") {
    return "/docs/getting-started.md";
  }

  if (pathname.endsWith("/index.md")) {
    return pathname.replace(/\/index\.md$/, "/getting-started.md");
  }

  if (pathname.endsWith(".md")) {
    return pathname;
  }

  if (pathname.endsWith(".html")) {
    return pathname.replace(/\.html$/, ".md");
  }

  if (pathname.split("/").pop().includes(".")) {
    return pathname;
  }

  if (pathname.endsWith("/")) {
    return `${pathname}getting-started.md`;
  }

  return `${pathname}.md`;
}
