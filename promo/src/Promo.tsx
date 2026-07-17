import React from "react";
import { AbsoluteFill, Series } from "remotion";
import { theme } from "./theme";
import { Hook } from "./scenes/Hook";
import { Problem } from "./scenes/Problem";
import { Manifest } from "./scenes/Manifest";
import { Sync } from "./scenes/Sync";
import { Build } from "./scenes/Build";
import { Test } from "./scenes/Test";
import { Verify } from "./scenes/Verify";
import { Plugins } from "./scenes/Plugins";
import { PluginNew } from "./scenes/PluginNew";
import { Cockpit } from "./scenes/Cockpit";
import { Payoff } from "./scenes/Payoff";

export const DURATIONS = {
  hook: 120,
  problem: 150,
  manifest: 150,
  sync: 150,
  build: 165,
  test: 170,
  verify: 105,
  plugins: 175,
  pluginNew: 140,
  cockpit: 165,
  payoff: 150,
};

export const TOTAL = Object.values(DURATIONS).reduce((a, b) => a + b, 0);

const scenes: [keyof typeof DURATIONS, React.FC][] = [
  ["hook", Hook],
  ["problem", Problem],
  ["manifest", Manifest],
  ["sync", Sync],
  ["build", Build],
  ["test", Test],
  ["verify", Verify],
  ["plugins", Plugins],
  ["pluginNew", PluginNew],
  ["cockpit", Cockpit],
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
