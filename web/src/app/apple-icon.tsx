import { ImageResponse } from "next/og";

export const dynamic = "force-static";
export const size = { width: 180, height: 180 };
export const contentType = "image/png";

export default function AppleIcon() {
  return new ImageResponse(
    (
      <div style={{ width: "100%", height: "100%", display: "flex", alignItems: "center", justifyContent: "center", background: "#0a0a0a" }}>
        <svg width="120" height="120" viewBox="0 0 24 24">
          <rect x="2.75" y="6.75" width="14.5" height="14.5" rx="3.25" fill="none" stroke="#fff" strokeWidth="1.5" strokeOpacity="0.45" />
          <rect x="7" y="2" width="15" height="15" rx="3.5" fill="#fff" />
          <path d="M14.5 13V6.5m-3 3 3-3 3 3" fill="none" stroke="#0a0a0a" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
    ),
    size,
  );
}
