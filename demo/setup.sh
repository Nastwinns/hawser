#!/usr/bin/env sh
# Builds a throwaway multi-repo workspace for the VHS demos (demo/*.tape).
# Sourced by the tapes so the `cd` into the workspace persists.

DEMO_LAB="/tmp/haw-demo"
rm -rf "$DEMO_LAB"
export GIT_AUTHOR_NAME="hawser Demo" GIT_AUTHOR_EMAIL="demo@hawser.dev"
export GIT_COMMITTER_NAME="hawser Demo" GIT_COMMITTER_EMAIL="demo@hawser.dev"

for repo in kernel hal app-mqtt; do
    mkdir -p "$DEMO_LAB/$repo"
    cd "$DEMO_LAB/$repo" || exit 1
    git init -q -b main
    git config user.email demo@hawser.dev
    git config user.name "hawser Demo"
    echo "$repo sources" > README.md
    git add . && git commit -qm "init $repo"
done

mkdir -p "$DEMO_LAB/gateway"
cd "$DEMO_LAB/gateway" || exit 1
cat > haw.toml <<MANIFEST
[repo.kernel]
url = "$DEMO_LAB/kernel"
rev = "main"
groups = ["firmware"]

[repo.hal]
url = "$DEMO_LAB/hal"
rev = "main"
groups = ["firmware"]

[repo.app-mqtt]
url = "$DEMO_LAB/app-mqtt"
rev = "main"

[stack.gateway]
repos = ["kernel", "hal", "app-mqtt"]

[stack.sensor-node]
repos = ["kernel", "hal"]
MANIFEST

# Optional helper for the merge demo (demo/cli-merge.tape). Not run by default,
# so the kernel/hal/app-mqtt + gateway lab used by cli.tape/tui.tape is untouched.
# Call it AFTER `haw sync` from the gateway workspace to seed a real conflict:
# `main` and `feature/tweak` both edit src/driver.c, so `haw merge plan` slices it.
haw_demo_setup_merge() {
    ( cd kernel || exit 1
      mkdir -p src
      printf 'line1\nline2\n' > src/driver.c
      git add src/driver.c && git commit -qm 'add driver'
      git checkout -q -b feature/tweak
      printf 'line1-FEATURE\nline2\n' > src/driver.c
      git commit -qam 'feature: tweak driver'
      git checkout -q main
      printf 'line1-MAIN\nline2\n' > src/driver.c
      git commit -qam 'main: tweak driver' )
}

clear
