# mobile — a mobile app pinned in lockstep with its networking SDK

Two genuine public mobile projects composed as one `haw` fleet: **OkHttp**
(Square's Kotlin HTTP client — the networking SDK under a huge slice of Android
apps) and **Now in Android** (Google's flagship sample app). The point of this
example is the **wiring**: an app and a library it depends on, pinned as one
fleet so they move together.

This host has **no Android SDK and no Xcode**, so the halves land differently —
and the manifest is honest about it:

| Repo | Role | Status on this host | Recipe |
| --- | --- | --- | --- |
| [OkHttp](https://github.com/square/okhttp) | SDK | **VALIDATED ✓ — builds + compiles tests for real** | `:okhttp:compileKotlinJvm` / `:okhttp:compileTestKotlinJvm` via `eclipse-temurin:21-jdk` |
| [Now in Android](https://github.com/android/nowinandroid) | app | **PATTERN — needs Android SDK** | `:app:assembleDebug` / `testDemoDebugUnitTest` |

OkHttp is pure Kotlin/JVM, so it builds with **no Android SDK at all**. It is
run inside a **JDK-21 Docker image** because gradle 9.6.1 requires a JDK-21
toolchain that cannot auto-provision on macOS/aarch64, and the host JDK is only
8. Now in Android needs the real Android SDK (`ANDROID_HOME` / `sdk.dir`); off
the SDK its Gradle build configures and `build-logic` compiles, then
`:app:assembleDebug` fails with **"SDK location not found"** — the expected,
honestly-marked pattern half.

## Run it

The `sdk-only` stack is the part that builds+tests on any host with Docker:

```console
$ mkdir /tmp/mobile && cp haw.toml /tmp/mobile/ && cd /tmp/mobile
$ haw sync --stack sdk-only --filter=blob:none   # clones OkHttp
$ haw build --group sdk                          # :okhttp:compileKotlinJvm
$ haw test  --group sdk                          # :okhttp:compileTestKotlinJvm
```

### Captured output (real, this host — fresh clone)

```console
$ haw sync --stack sdk-only --filter=blob:none
  ✓ okhttp  cloned
synced stack `sdk-only` (1/1 repos)

$ haw build --group sdk
...
BUILD SUCCESSFUL in 1m 48s
build ran in 1/1 repos

$ haw test --group sdk
...
BUILD SUCCESSFUL in 49s
test ran in 1/1 repos
```

The full `fleet` stack also clones Now in Android; on an Android-SDK host its
`build`/`test` targets (already the real ones in `haw.toml`) run as-is.

### What "needs Android SDK" looks like (captured)

```console
$ ./gradlew :app:assembleDebug        # nowinandroid, off-SDK
> Task :build-logic:convention:jar
* What went wrong:
> SDK location not found. Define a valid SDK location with an ANDROID_HOME
  environment variable or by setting the sdk.dir path ...
BUILD FAILED
```

## Prerequisites

- **Network** — `haw sync` clones both upstreams from GitHub.
- **Docker** — daemon running. The OkHttp recipes run gradle inside
  `eclipse-temurin:21-jdk`; a `haw-gradle-cache` volume is mounted so re-runs
  reuse the downloaded gradle + dependencies.
- **Now in Android (pattern)** — needs the Android SDK (command-line tools +
  a platform), i.e. `ANDROID_HOME` set or `local.properties` with `sdk.dir`.
  Not available on this host, so its recipe is kept as a pattern.

See also the [examples index](../README.md).
