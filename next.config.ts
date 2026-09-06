import type { NextConfig } from "next";

/**
 * The UI is a thin client over `ember-server`. There are two ways it runs:
 *
 * - Production / desktop (`EMBER_EXPORT=1 next build`): a static export that
 *   `ember-server` serves itself, so the API is same-origin. Static export is
 *   what the Tauri shell needs, and it rules out rewrites and any route
 *   handler that reads a Request or a cookie — which is why the old
 *   `app/api/*` routes moved into Rust.
 *
 * - Development (`next dev`): the full dev server with hot reload, proxying
 *   `/api/*` to the Rust server. Proxying keeps requests same-origin in dev
 *   too, so the session cookie behaves exactly as it will in production and
 *   no CORS configuration is needed.
 */

const emberApi = process.env.EMBER_API ?? "http://127.0.0.1:4000";

const nextConfig: NextConfig = process.env.EMBER_EXPORT === "1"
  ? { output: "export" }
  : {
      rewrites: () => [
        { source: "/api/:path*", destination: `${emberApi}/api/:path*` },
      ],
    };

export default nextConfig;
