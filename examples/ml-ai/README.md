# ml-ai — a real AI runtime, built from source and pinned with a training repo

Two genuine public ML/AI projects composed as one `haw` fleet: **llama.cpp**,
the reference C/C++ LLM inference runtime, compiled from source into a working
`llama-cli`; and **nanoGPT**, Karpathy's minimal GPT training repo. Every
`build =` / `test =` in [`haw.toml`](haw.toml) was **actually executed with
`haw` and seen to succeed** on this host (cmake + AppleClang, python3).

This is the same "runtime + pipeline pinned as one reproducible baseline"
idea as the reading-only [`ml-platform/`](../ml-platform/) example — but here
the runtime is a **real AI inference engine built from source**, not an
illustrative URL.

## The fleet

| Repo | Domain | build (validated ✓) | test (validated ✓) |
| --- | --- | --- | --- |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | runtime | CMake build of ggml + `llama-cli` from C/C++ source (`-DLLAMA_CURL=OFF`) — **`[100%] Built target llama-cli`** | `./build/bin/llama-cli --version` → **`version: 10064 (86d86ed43)` / `built with AppleClang 17.0.0`** |
| [nanoGPT](https://github.com/karpathy/nanoGPT) | training | `python3 -m py_compile *.py` — byte-compiles every module | AST-parse every `.py` → **`NANOGPT_PARSE_OK`** |

Both green: `build ran in 2/2 repos`, `test ran in 2/2 repos` (exit 0).

## Run it

```console
$ mkdir /tmp/mlai && cp haw.toml /tmp/mlai/ && cd /tmp/mlai
$ haw sync --filter=blob:none   # clones both upstreams (needs network)
$ haw build -j4                 # compiles llama.cpp, py_compiles nanoGPT
$ haw test                      # llama-cli --version, parse-checks nanoGPT
```

### Captured output (real, this host)

```console
$ haw build -j4
...
[100%] Built target llama-cli
── nanogpt ──
build ran in 2/2 repos

$ haw test
── llamacpp ──
version: 10064 (86d86ed43)
built with AppleClang 17.0.0.17000013 for Darwin arm64
── nanogpt ──
NANOGPT_PARSE_OK
test ran in 2/2 repos
```

## Prerequisites

- **Network** — `haw sync` clones both upstreams from GitHub.
- **C/C++ toolchain + CMake** — for the llama.cpp build (clang/gcc, `cmake`).
  The first build compiles all of ggml and takes a few minutes; it is
  incremental thereafter.
- **python3** — for the nanoGPT parse-checks (no torch/numpy needed for these).

### Pattern (not run here): actually train / run inference

The recipes above compile and smoke-test. To go further you would add heavier
recipes that this host did not run:

- **Run inference** — `./build/bin/llama-cli -m <model.gguf> -p "hello"`
  needs a downloaded GGUF model (pattern — needs a model file).
- **Train nanoGPT** — `python3 train.py` needs `torch`, `numpy`, `tiktoken`
  and a prepared dataset (pattern — needs PyTorch + dataset).

See also the [examples index](../README.md).
