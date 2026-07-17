import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { theme } from "../theme";
import { Terminal } from "../components/Terminal";
import { fadeIn, typed } from "../components/anim";

// `haw plugins new my-check --lang python` scaffolds a runnable haw-<name> dir.
const tree = [
  { t: "haw-my-check/", c: theme.accent },
  { t: "├── haw-my-check       # runnable entrypoint (haw my-check)", c: theme.text },
  { t: "├── plugin.py          # reads haw.plugin/1, writes report/1", c: theme.dim },
  { t: "└── README.md", c: theme.dim },
];

export const PluginNew: React.FC = () => {
  const frame = useCurrentFrame();
  const cmd = typed("haw plugins new my-check --lang python", frame, 4, 34);

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
          marginBottom: 26,
          opacity: fadeIn(frame, 2),
        }}
      >
        …or write your own —{" "}
        <span style={{ color: theme.accent }}>Rust · Python · Go · shell</span>
      </div>

      <Terminal title="haw plugins new" width={1180}>
        <div style={{ fontSize: 24, lineHeight: 1.6 }}>
          <div style={{ color: theme.text }}>
            <span style={{ color: theme.green }}>$ </span>
            {cmd}
            {frame < 40 && <span style={{ color: theme.accent }}>▋</span>}
          </div>
          <div
            style={{
              marginTop: 10,
              color: theme.green,
              opacity: fadeIn(frame, 46),
            }}
          >
            ✓ scaffolded a runnable haw-my-check skeleton
          </div>
          <div style={{ marginTop: 14 }}>
            {tree.map((l, i) => (
              <div
                key={i}
                style={{ color: l.c, opacity: fadeIn(frame, 56 + i * 10) }}
              >
                {l.t}
              </div>
            ))}
          </div>
          <div
            style={{
              marginTop: 18,
              color: theme.dim,
              fontSize: 21,
              opacity: fadeIn(frame, 100),
            }}
          >
            a subprocess speaking a JSON contract — no fork, no rebuild of haw.
          </div>
        </div>
      </Terminal>
    </AbsoluteFill>
  );
};
