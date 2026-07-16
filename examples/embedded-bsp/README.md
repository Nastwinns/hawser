# embedded-bsp — a Zephyr-style firmware / BSP stack

A board-support-package fleet laid out the way a Zephyr `west` workspace is: the
kernel plus HAL and bootloader modules checked out at fixed paths. It shows how
haw mixes **pinned** and **branch-tracking** repos and applies overlays. This is
a *reading* example — inspect it with `--manifest`.

## What it demonstrates

- **Fixed checkout paths.** `hal_stm32` lands at `modules/hal/stm32` and
  `mcuboot` at `bootloader/mcuboot` via `path =`, reproducing the west layout so
  the build system finds modules where it expects them.
- **Pinned vs floating revs.** The kernel is pinned to tag `v3.6.0` (already
  reproducible); the HAL and bootloader track `main` and *follow their head*
  until `haw lock` pins them to a SHA. Run `haw lock`, commit `haw.lock`, and the
  whole tree becomes reproducible.
- **Two stacks.** `stm32-board` is the full target (kernel + HAL + bootloader);
  `qemu-sim` is just the kernel for host simulation.
- **Groups + `deps`.** `core` / `hal` / `boot` groups; the HAL and bootloader
  declare `deps = ["zephyr"]` so land/build order respects the kernel.
- **An overlay.** `bleeding-edge` re-points the kernel at `main`, so
  `haw lock --overlay bleeding-edge` resolves against upstream head — track the
  bleeding edge without editing the committed baseline.
- **`build =` per repo.** The kernel carries a `west build …` command and the
  bootloader a `make …`, so `haw build` drives the toolchain per repo.

## Inspect it

```console
$ haw tree --manifest examples/embedded-bsp/haw.toml
examples/embedded-bsp/haw.toml
├─ stm32-board
│  ├─ zephyr     v3.6.0  (https://github.com/zephyrproject-rtos/zephyr)
│  ├─ hal_stm32  main  (https://github.com/zephyrproject-rtos/hal_stm32)
│  └─ mcuboot    main  (https://github.com/zephyrproject-rtos/mcuboot)
└─ qemu-sim
   └─ zephyr  v3.6.0  (https://github.com/zephyrproject-rtos/zephyr)
```

## The workflow it models

```console
$ haw sync --stack stm32-board       # clone kernel + modules at their paths
$ haw lock                            # pin the main-tracking modules to SHAs
$ haw build --group core              # west-build the kernel
$ haw build                           # then HAL + bootloader
# track upstream without touching the baseline:
$ haw lock --overlay bleeding-edge    # re-resolve the kernel on main
```

> These are the actual `zephyrproject-rtos` repos, but the stack is large and
> assumes a Zephyr toolchain, so this example is meant for reading. For a small
> repo set you can clone and build in seconds, use
> [`../quickstart`](../quickstart/).

See also: [`../automotive-pinned`](../automotive-pinned/) (fully pinned baseline)
and the [examples index](../README.md).
