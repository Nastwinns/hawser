# Renders demo/haw-tui.gif — the k9s-style fleet cockpit.
# Driven by the built-in demo controller: no workspace, no git, no network.
# Regenerate locally with: vhs demo/tui.tape   (needs the release binary built)

Output demo/haw-tui.gif

Set FontSize 16
Set Width 1100
Set Height 620
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms

Hide
Type `export PATH="$PWD/target/release:$PATH"`
Enter
Type "clear"
Enter
Show

Type "haw dash --demo"
Enter
Sleep 3s

# Walk the fleet grid — clean, dirty, drift, and a not-cloned repo.
Type "j"
Sleep 1s
Type "j"
Sleep 1.5s
Type "j"
Sleep 1.5s
Type "k"
Sleep 1s

# Filter to one repo, then clear the filter.
Type "/"
Sleep 500ms
Type "hal"
Sleep 1.5s
Enter
Sleep 1.5s
Escape
Sleep 1s

# Fleet-wide open PR/MRs across every repo (github + gitlab).
Type "m"
Sleep 2.5s
Type "j"
Sleep 800ms
Type "j"
Sleep 1s
Type "b"
Sleep 800ms

# Recent CI runs across every repo (passed/failed/running/queued/cancelled).
Type "i"
Sleep 2.5s
Type "j"
Sleep 800ms
Type "j"
Sleep 1s
Type "b"
Sleep 800ms

# Governance — registered plugins, SBOM/provenance artifacts, findings.
Type "v"
Sleep 2.5s
Type "j"
Sleep 800ms
Type "b"
Sleep 800ms

# Changesets — drill into one to show its PR/CI cells per repo.
Type "c"
Sleep 2s
Enter
Sleep 2.5s
Type "b"
Sleep 800ms
Type "b"
Sleep 800ms

# Help overlay — the full keymap.
Type "?"
Sleep 2.5s
Escape
Sleep 1s

Type "q"
Sleep 1.5s
