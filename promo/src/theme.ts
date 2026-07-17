export const theme = {
  bg: "#0d1117",
  bgSoft: "#161b22",
  panel: "#1c2128",
  border: "#30363d",
  text: "#e6edf3",
  dim: "#8b949e",
  accent: "#a371f7",
  accentDeep: "#8A2BE2",
  green: "#3fb950",
  amber: "#d29922",
  red: "#f85149",
  blue: "#58a6ff",
  mono: "'SF Mono', 'JetBrains Mono', 'Fira Code', 'Menlo', monospace",
  sans:
    "'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
};

// Real data captured from examples/embedded-real (every command was executed).
export const fleet = [
  {
    name: "coremark",
    repo: "eembc/coremark",
    group: "benchmark",
    sha: "1f483d5",
    result: "CoreMark 1.0 : 26021.34",
    ok: "COREMARK_RAN",
    color: "#58a6ff",
  },
  {
    name: "cjson",
    repo: "DaveGamble/cJSON",
    group: "data",
    sha: "fb16e5c",
    result: "100% tests passed (19)",
    ok: "ctest OK",
    color: "#3fb950",
  },
  {
    name: "monocypher",
    repo: "LoupVaillant/Monocypher",
    group: "crypto",
    sha: "ab2b16d",
    result: "libmonocypher.a",
    ok: "MONOCYPHER_LIB_OK",
    color: "#a371f7",
  },
  {
    name: "libcanard",
    repo: "OpenCyphal/libcanard",
    group: "protocol",
    sha: "1206003",
    result: "C11 -fsyntax-only",
    ok: "LIBCANARD_SYNTAX_OK",
    color: "#d29922",
  },
  {
    name: "mbedtls",
    repo: "Mbed-TLS/mbedtls",
    group: "security",
    sha: "c848d22",
    result: "libmbedcrypto.a",
    ok: "libmbed*.a",
    color: "#f85149",
  },
];

// Real `haw plugins list` catalog (captured from the 0.1.7 binary).
export const plugins = [
  { name: "artifact", desc: "SLSA/in-toto provenance + cosign/minisign signing" },
  { name: "aspice", desc: "ASPICE/qualification traceability from the pinned fleet" },
  { name: "compliance", desc: "SBOM (CycloneDX + SPDX) generation" },
  { name: "git-gate", desc: "secret / hygiene pre-commit & lifecycle gate" },
  { name: "jira", desc: "link a changeset to a Jira issue, transition on land" },
  { name: "misra", desc: "MISRA C static-analysis gate (cppcheck)" },
];

// Real cargo command printed by `haw plugins install aspice --dry-run`.
export const installCmd =
  "cargo install --locked --git https://github.com/Nastwinns/hawser --tag v0.1.7 haw-aspice";
