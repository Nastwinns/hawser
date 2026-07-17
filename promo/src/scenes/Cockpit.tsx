import React from "react";
import { AbsoluteFill, useCurrentFrame, interpolate } from "remotion";
import { theme, fleet } from "../theme";
import { fadeIn } from "../components/anim";

const cols = ["REPO", "BRANCH", "HEAD", "DIRTY", "DRIFT", "PR", "CI"];
const rows = fleet.map((r, i) => ({
  repo: r.name,
  branch: i === 2 ? "feat/x25519" : "main",
  head: r.sha,
  dirty: i === 2 ? "●" : "·",
  drift: i === 4 ? "⚠" : "·",
  pr: i === 2 ? "#128" : "—",
  ci: i === 4 ? "fail" : "pass",
}));

export const Cockpit: React.FC = () => {
  const frame = useCurrentFrame();
  // selection cursor sweeps down
  const sel = Math.min(
    rows.length - 1,
    Math.floor(interpolate(frame, [30, 120], [0, rows.length - 1], {
      extrapolateLeft: "clamp",
      extrapolateRight: "clamp",
    }))
  );

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
          fontSize: 34,
          color: theme.dim,
          marginBottom: 22,
          opacity: fadeIn(frame, 2),
        }}
      >
        drive the whole fleet from one cockpit — bare{" "}
        <span style={{ color: theme.accent }}>haw</span>
      </div>

      <div
        style={{
          width: 1320,
          background: theme.bgSoft,
          border: `1px solid ${theme.border}`,
          borderRadius: 12,
          overflow: "hidden",
          boxShadow: "0 40px 120px rgba(0,0,0,.55)",
          opacity: fadeIn(frame, 8),
        }}
      >
        <div
          style={{
            padding: "12px 20px",
            background: theme.panel,
            borderBottom: `1px solid ${theme.border}`,
            color: theme.accent,
            fontSize: 22,
            display: "flex",
            justifyContent: "space-between",
          }}
        >
          <span>haw · fleet [5]</span>
          <span style={{ color: theme.dim }}>context: embedded-real</span>
        </div>

        <div style={{ padding: "14px 24px", fontSize: 24 }}>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "1.6fr 1.6fr 1fr .8fr .8fr .8fr .8fr",
              color: theme.dim,
              letterSpacing: 1,
              paddingBottom: 8,
              borderBottom: `1px solid ${theme.border}`,
            }}
          >
            {cols.map((c) => (
              <span key={c}>{c}</span>
            ))}
          </div>

          {rows.map((r, i) => {
            const active = i === sel;
            return (
              <div
                key={r.repo}
                style={{
                  display: "grid",
                  gridTemplateColumns: "1.6fr 1.6fr 1fr .8fr .8fr .8fr .8fr",
                  padding: "9px 0",
                  background: active ? "rgba(163,113,247,0.16)" : "transparent",
                  borderLeft: `3px solid ${active ? theme.accent : "transparent"}`,
                  paddingLeft: 10,
                  opacity: fadeIn(frame, 14 + i * 6),
                }}
              >
                <span style={{ color: theme.text, fontWeight: 600 }}>{r.repo}</span>
                <span
                  style={{ color: r.branch === "main" ? theme.dim : theme.blue }}
                >
                  {r.branch}
                </span>
                <span style={{ color: theme.blue }}>{r.head}</span>
                <span style={{ color: r.dirty === "●" ? theme.amber : theme.dim }}>
                  {r.dirty}
                </span>
                <span style={{ color: r.drift === "⚠" ? theme.red : theme.dim }}>
                  {r.drift}
                </span>
                <span style={{ color: r.pr === "—" ? theme.dim : theme.accent }}>
                  {r.pr}
                </span>
                <span
                  style={{ color: r.ci === "pass" ? theme.green : theme.red }}
                >
                  {r.ci}
                </span>
              </div>
            );
          })}
        </div>

        <div
          style={{
            padding: "10px 24px",
            borderTop: `1px solid ${theme.border}`,
            color: theme.dim,
            fontSize: 20,
            background: theme.panel,
          }}
        >
          <span style={{ color: theme.accent }}>:</span> command ·{" "}
          <span style={{ color: theme.text }}>/</span> filter ·{" "}
          <span style={{ color: theme.text }}>enter</span> drill in ·{" "}
          <span style={{ color: theme.text }}>d</span> diff ·{" "}
          <span style={{ color: theme.text }}>m</span> merge ·{" "}
          <span style={{ color: theme.red }}>⚠</span> problems-only
        </div>
      </div>
    </AbsoluteFill>
  );
};
