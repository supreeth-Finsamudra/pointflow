import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Build to static HTML/JS so the Rust agent can serve the phone UI over WiFi.
  output: "export",
  // Emit /route/index.html so a plain static file server resolves cleanly.
  trailingSlash: true,
  images: { unoptimized: true },
};

export default nextConfig;
