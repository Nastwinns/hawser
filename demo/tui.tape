# Renders demo/haw-tui.gif — the k9s-style fleet cockpit.
# Regenerate locally with: vhs demo/tui.tape   (needs the release binary built)

Output demo/haw-tui.gif

Set FontSize 16
Set Width 1100
Set Height 620
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms

Hide
Type `export PATH="$PWD/target/release:$PATH" && . demo/setup.sh && haw sync --stack gateway >/dev/null && echo tweak >> hal/README.md && clear`
Enter
Sleep 2s
Show

Type "haw"
Enter
Sleep 3s

# Walk the fleet grid.
Type "j"
Sleep 800ms
Type "j"
Sleep 800ms
Type "k"
Sleep 800ms

# Filter, then clear the filter.
Type "/"
Sleep 500ms
Type "hal"
Sleep 1s
Enter
Sleep 1.5s
Escape
Sleep 800ms

# Fleet-wide open PR/MRs across every repo.
Type "m"
Sleep 2s
Type "b"
Sleep 600ms

# Recent CI runs across every repo.
Type "i"
Sleep 2s
Type "b"
Sleep 600ms

# Changesets view.
Type "c"
Sleep 2s
Type "b"
Sleep 600ms

# Help overlay — the full keymap.
Type "?"
Sleep 2.5s
Escape
Sleep 1s

Type "q"
Sleep 1.5s
