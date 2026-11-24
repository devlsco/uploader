import type { NextConfig } from "next";

const dev = process.env.NODE_ENV === "development";

const nextConfig: NextConfig = {
  output: "export",
  poweredByHeader: false,
  images: { unoptimized: true },
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
  ...(dev && {
    async rewrites() {
      const api = process.env.UPLOADER_DEV_API ?? "http://127.0.0.1:3000";
      return [{ source: "/api/:path*", destination: `${api}/api/:path*` }];
    },
  }),
};

export default nextConfig;
