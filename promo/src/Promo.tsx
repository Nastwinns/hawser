import React from "react";
import { AbsoluteFill, Series } from "remotion";
import { theme } from "./theme";
import { Hook } from "./scenes/Hook";
import { Problem } from "./scenes/Problem";
import { Manifest } from "./scenes/Manifest";
import { Sync } from "./scenes/Sync";
import { Verify } from "./scenes/Verify";
import { Plugins } from "./scenes/Plugins";
import { PluginNew } from "./scenes/PluginNew";
import { Payoff } from "./scenes/Payoff";
import { RealClip } from "./scenes/RealClip";

// Real VHS captures live in promo/public — durations (30fps) sized to each clip.
const RealBuild: React.FC = () => (
  <RealClip
    src="promo-build.mp4"
    step="4"
    title={
      <>
        build the whole fleet in parallel — <span style={{ color: theme.accent }}>haw build -j4</span>
      </>
    }
  />
);
const RealTest: React.FC = () => (
  <RealClip
    src="promo-test.mp4"
    step="5"
    title={
      <>
        real toolchains actually run — <span style={{ color: theme.accent }}>haw test</span>
      </>
    }
  />
);
const RealTui: React.FC = () => (
  <RealClip
    src="promo-tui.mp4"
    step="8"
    title={
      <>
        drive the fleet from the cockpit — bare <span style={{ color: theme.accent }}>haw</span>
      </>
    }
  />
);

export const DURATIONS = {
  hook: 120,
  problem: 150,
  manifest: 150,
  sync: 150,
  build: 458,
  test: 279,
  verify: 105,
  plugins: 175,
  pluginNew: 140,
  cockpit: 396,
  payoff: 150,
};

export const TOTAL = Object.values(DURATIONS).reduce((a, b) => a + b, 0);

const scenes: [keyof typeof DURATIONS, React.FC][] = [
  ["hook", Hook],
  ["problem", Problem],
  ["manifest", Manifest],
  ["sync", Sync],
  ["build", RealBuild],
  ["test", RealTest],
  ["verify", Verify],
  ["plugins", Plugins],
  ["pluginNew", PluginNew],
  ["cockpit", RealTui],
  ["payoff", Payoff],
];

export const Promo: React.FC = () => {
  return (
    <AbsoluteFill style={{ background: theme.bg }}>
      <Series>
        {scenes.map(([key, Comp]) => (
          <Series.Sequence key={key} durationInFrames={DURATIONS[key]}>
            <Comp />
          </Series.Sequence>
        ))}
      </Series>
    </AbsoluteFill>
  );
};
