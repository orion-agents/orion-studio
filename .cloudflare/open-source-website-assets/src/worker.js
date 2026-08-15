export default {
  async fetch(request, env) {
    if (!isSafeMethod(request.method)) {
      return methodNotAllowed();
    }

    const bucket = env?.ORION_STUDIO_OPEN_SOURCE_WEBSITE_ASSETS_BUCKET;
    const websiteOrigin = configuredOrigin(env, "ORION_STUDIO_WEBSITE_ORIGIN");
    if (!bucket || typeof bucket.get !== "function" || !websiteOrigin) {
      return configurationError("Orion Studio website assets worker configuration is incomplete.");
    }

    const url = new URL(request.url);
    if (url.pathname !== "/install.sh") {
      return new Response("Not Found", { status: 404 });
    }
    const key = url.pathname.slice(1);

    const object = await bucket.get(key);
    if (!object) {
      const fallbackUrl = new URL(websiteOrigin);
      fallbackUrl.pathname = "/404";
      return fetch(safeUpstreamRequest(request, fallbackUrl));
    }

    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("etag", object.httpEtag);

    headers.set("x-content-type-options", "nosniff");

    return new Response(request.method === "HEAD" ? null : object.body, {
      headers,
    });
  },
};

function configuredOrigin(env, name) {
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
  const accept = request.headers.get("accept");
  if (accept !== null) {
    headers.set("accept", accept);
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
