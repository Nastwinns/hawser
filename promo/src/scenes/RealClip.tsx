import React from "react";
import {
  AbsoluteFill,
  OffthreadVideo,
  staticFile,
  useCurrentFrame,
} from "remotion";
import { theme } from "../theme";
import { fadeIn } from "../components/anim";

// Embeds a REAL VHS terminal capture (not a re-enactment) with a title banner.
export const RealClip: React.FC<{
  src: string;
  step: string;
  title: React.ReactNode;
  badge?: string;
}> = ({ src, step, title, badge = "real capture" }) => {
  const frame = useCurrentFrame();

  return (
    <AbsoluteFill
      style={{
        background: theme.bg,
        fontFamily: theme.mono,
        justifyContent: "center",
        alignItems: "center",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 16,
          marginBottom: 22,
          opacity: fadeIn(frame, 2),
        }}
      >
        <span style={{ fontSize: 34, color: theme.dim }}>
          {step} — {title}
        </span>
        <span
          style={{
            fontSize: 20,
            color: theme.green,
            border: `1px solid ${theme.green}`,
            borderRadius: 20,
            padding: "3px 14px",
          }}
        >
          ● {badge}
        </span>
      </div>

      <div
        style={{
          width: 1440,
          borderRadius: 14,
          overflow: "hidden",
          border: `1px solid ${theme.border}`,
          boxShadow: "0 40px 120px rgba(0,0,0,.55)",
          opacity: fadeIn(frame, 6),
          background: "#1e1e2e",
        }}
      >
        <OffthreadVideo
          src={staticFile(src)}
          muted
          style={{ width: "100%", display: "block" }}
        />
      </div>
    </AbsoluteFill>
  );
};
