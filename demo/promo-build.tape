# Renders demo/promo-build-raw.mp4 — REAL parallel build of the embedded-real quick
# stack (coremark, cJSON, monocypher, libcanard). Real toolchains, real output.
# The raw capture is ~90s (coremark re-runs its benchmark); sped up ~5x in post.
#
# Prep once:  mkdir -p /tmp/embq && cp examples/embedded-real/haw.toml /tmp/embq/
#             ( cd /tmp/embq && haw sync --stack quick )
# Then:       vhs demo/promo-build.tape
#             ffmpeg -i demo/promo-build-raw.mp4 -vf "setpts=PTS/5" -an demo/promo-build.mp4

Output demo/promo-build-raw.mp4

Set FontSize 17
Set Width 1280
Set Height 720
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 55ms

Hide
Type `export PATH="$PWD/target/release:$PATH" CLICOLOR_FORCE=1; cd /tmp/embq`
Enter
Type "clear"
Enter
Show

Sleep 700ms
Type "# four real embedded upstreams — build the whole fleet in parallel"
Enter
Type "haw build -j4"
Sleep 300ms
Enter
Sleep 90s
