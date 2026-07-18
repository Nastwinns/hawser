# Renders demo/promo-test.mp4 — REAL `haw test` on the embedded-real quick stack.
# Instant (greps run1.log, runs ctest): CoreMark score, cJSON 19/19, Monocypher archive.
#
# Prep once (build first so the test recipes have artifacts):
#   ( cd /tmp/embq && haw build -j4 )
# Then: vhs demo/promo-test.tape

Output demo/promo-test.mp4

Set FontSize 18
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
Type "# real test recipes across the fleet — one CI-grade exit code"
Enter
Type "haw test"
Sleep 300ms
Enter
Sleep 6s
