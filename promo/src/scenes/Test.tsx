import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { theme } from "../theme";
import { Terminal } from "../components/Terminal";
import { fadeIn, typed } from "../components/anim";

// Verbatim from examples/embedded-real "Captured output (real)".
const lines: { t: string; c: string; b?: boolean }[] = [
  { t: "── coremark ──", c: theme.dim },
  {
    t: "CoreMark 1.0 : 26021.337497 / Apple LLVM 17.0.0 -O2 -DPERFORMANCE_RUN=1 / Heap",
    c: theme.text,
  },
  { t: "COREMARK_RAN", c: theme.green, b: true },
  { t: "── cjson ──", c: theme.dim },
  { t: "100% tests passed out of 19", c: theme.green, b: true },
  { t: "── monocypher ──", c: theme.dim },
  { t: "MONOCYPHER_LIB_OK", c: theme.green, b: true },
];

export const Test: React.FC = () => {
  const frame = useCurrentFrame();
  const cmd = typed("haw test", frame, 4, 16);

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
        …and the real toolchains actually run —{" "}
        <span style={{ color: theme.accent }}>haw test</span>
      </div>

      <Terminal title="haw test — captured, real output" width={1280}>
        <div style={{ fontSize: 24, lineHeight: 1.55 }}>
          <div style={{ color: theme.text }}>
            <span style={{ color: theme.green }}>$ </span>
            {cmd}
            {frame < 24 && <span style={{ color: theme.accent }}>▋</span>}
          </div>
          {lines.map((l, i) => (
            <div
              key={i}
              style={{
                marginTop: 8,
                color: l.c,
                fontWeight: l.b ? 700 : 400,
                opacity: fadeIn(frame, 28 + i * 12),
              }}
            >
              {l.t}
            </div>
          ))}
          <div
            style={{
              marginTop: 16,
              fontSize: 25,
              color: theme.green,
              fontWeight: 700,
              opacity: fadeIn(frame, 118),
            }}
          >
            test ran in 3/3 repos
          </div>
        </div>
      </Terminal>
    </AbsoluteFill>
  );
};
