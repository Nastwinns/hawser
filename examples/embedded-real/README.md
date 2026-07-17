# embedded-real — a fleet of real embedded / safety-critical upstreams

Five genuine public embedded projects, composed as one `haw` fleet. Every
`build =` / `test =` in [`haw.toml`](haw.toml) was **actually executed with
`haw` and seen to succeed** — this is a runnable manifest, not a reading one.
`haw sync` clones the real upstreams over HTTPS with no credentials.

Unlike [`quickstart/`](../quickstart/) (which just clones), this fleet shows
`haw build` / `haw test` driving five heterogeneous, real toolchains at once —
`make`, `cmake`+`ctest`, a static-library build, and a C11 compile-check — with
one CI-grade exit code for the whole fleet.

## The fleet

| Repo | Domain | What builds | Test |
| --- | --- | --- | --- |
| [coremark](https://github.com/eembc/coremark) | benchmark | `make` compiles **and runs** the iconic embedded CPU benchmark, printing a CoreMark score + CRC self-check | `run1.log` shows `Correct operation validated` |
| [cJSON](https://github.com/DaveGamble/cJSON) | data | CMake build of the ubiquitous embedded JSON parser | `ctest` — **19/19 pass** |
| [Monocypher](https://github.com/LoupVaillant/Monocypher) | crypto | `make static-library` → `libmonocypher.a` (compact embedded crypto) | archive present (build-only¹) |
| [libcanard](https://github.com/OpenCyphal/libcanard) | protocol | C11 `-fsyntax-only` compile-check of OpenCyphal's UAVCAN/DroneCAN CAN stack | build-only |
| [Mbed-TLS](https://github.com/Mbed-TLS/mbedtls) | security | CMake build of the embedded TLS libraries (`libmbedcrypto/tls/x509.a`) | build-only² |

¹ Monocypher's `make test` needs libsodium to generate test vectors (not
available offline), so this recipe builds the library only.
² Mbed-TLS needs its git **submodules** and two Python codegen deps — see below.

## Run it

```console
$ mkdir /tmp/emb && cp haw.toml /tmp/emb/ && cd /tmp/emb
$ haw sync            # clones all five upstreams (needs network)
$ haw build -j4       # builds all five in parallel
$ haw test            # runs coremark, cJSON, monocypher
```

Or sync just the host-only slice (no submodules, no Python) — `haw build`/`test`
then operate on the repos that are checked out:

```console
$ haw sync --stack quick    # clones coremark, cJSON, monocypher, libcanard
$ haw build -j4
$ haw test
```

### Captured output (real)

```console
$ haw build -j4
...
build ran in 5/5 repos

$ haw test
── coremark ──
CoreMark 1.0 : 26021.337497 / Apple LLVM 17.0.0 (clang-1700.0.13.5) -O2 -DPERFORMANCE_RUN=1   / Heap
COREMARK_RAN
── cjson ──
100% tests passed out of 19
── monocypher ──
MONOCYPHER_LIB_OK
test ran in 3/3 repos
```

## Prerequisites

- **Network** — `haw sync` clones the five upstreams from GitHub.
- **Host toolchain** — clang/cc, `make`, `cmake`, `ctest` (the `quick` stack
  needs only these).
- **Mbed-TLS extras** (for the full `fleet` stack):
  - Submodules: `haw sync --recurse-submodules` initializes `framework/` and
    `tf-psa-crypto/`. haw's submodule sync is fault-tolerant — a broken or
    unreachable submodule is skipped with a warning instead of aborting the
    whole sync.
  - Python codegen deps `jinja2` + `jsonschema`. Install them (a venv is
    cleanest) and point CMake at that interpreter, e.g. append
    `-DPython3_EXECUTABLE=/path/to/venv/bin/python3` to the `mbedtls` `build =`.

## Bonus: host + cross in one fleet

The same cJSON checkout cross-compiles to a bare-metal **Cortex-M4** object with
the `haw-arm-emu` Docker image (arm-none-eabi-gcc) — showing a host build and a
cross build side by side. Executed and confirmed (`architecture: armv7e-m`):

```toml
build = "docker run --rm -v \"$PWD\":/w -w /w haw-arm-emu sh -c 'arm-none-eabi-gcc -mcpu=cortex-m4 -mthumb -Os -ffreestanding -c cJSON.c -o cJSON-cm4.o && arm-none-eabi-ar rcs libcjson-cm4.a cJSON-cm4.o && arm-none-eabi-objdump -f cJSON-cm4.o | grep -i \"architecture: arm\" && echo CJSON_CORTEXM4_OK'"
```

See [`../../docs/INTEGRATION.md`](../../docs/INTEGRATION.md) for the Docker/QEMU
cross-compile recipes (littlefs on Cortex-M4, FreeRTOS booted under QEMU).

See also the [examples index](../README.md).
