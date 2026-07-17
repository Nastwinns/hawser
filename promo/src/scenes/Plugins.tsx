import React from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { theme, plugins, installCmd } from "../theme";
import { Terminal } from "../components/Terminal";
import { fadeIn, typed } from "../components/anim";

export const Plugins: React.FC = () => {
  const frame = useCurrentFrame();
  const cmd = typed("haw plugins list", frame, 4, 22);
  const installStart = 120;
  const install = typed(
    "haw plugins install aspice",
    frame,
    installStart,
    26
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
          marginBottom: 26,
          opacity: fadeIn(frame, 2),
        }}
      >
        extend it — <span style={{ color: theme.accent }}>haw plugins</span>{" "}
        (governance, SBOM, ASPICE, MISRA…)
      </div>

      <Terminal title="haw plugins" width={1320}>
        <div style={{ fontSize: 23, lineHeight: 1.5 }}>
          <div style={{ color: theme.text }}>
            <span style={{ color: theme.green }}>$ </span>
            {cmd}
            {frame < 26 && <span style={{ color: theme.accent }}>▋</span>}
          </div>

          <div style={{ marginTop: 12, opacity: fadeIn(frame, 30) }}>
            <div style={{ color: theme.dim, letterSpacing: 1 }}>
              {"NAME        STATUS     DESCRIPTION"}
            </div>
            {plugins.map((p, i) => (
              <div
                key={p.name}
                style={{ opacity: fadeIn(frame, 36 + i * 8), marginTop: 4 }}
              >
                <span
                  style={{
                    color: theme.accent,
                    display: "inline-block",
                    width: 150,
                  }}
                >
                  {p.name}
                </span>
                <span
                  style={{
                    color: theme.green,
                    display: "inline-block",
                    width: 130,
                  }}
                >
                  available
                </span>
                <span style={{ color: theme.dim }}>{p.desc}</span>
              </div>
            ))}
          </div>

          <div style={{ marginTop: 22, opacity: fadeIn(frame, installStart) }}>
            <span style={{ color: theme.green }}>$ </span>
            <span style={{ color: theme.text }}>{install}</span>
            {frame >= installStart && frame < installStart + 30 && (
              <span style={{ color: theme.accent }}>▋</span>
            )}
          </div>
          <div
            style={{
              marginTop: 8,
              color: theme.blue,
              fontSize: 21,
              opacity: fadeIn(frame, installStart + 30),
            }}
          >
            $ {installCmd}
          </div>
        </div>
      </Terminal>
    </AbsoluteFill>
  );
};
