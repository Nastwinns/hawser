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
Sleep 2.5s

# Walk the fleet grid — clean, dirty, drift, a not-cloned repo.
Type "j"
Sleep 700ms
Type "j"
Sleep 900ms

# Drill into a repo: branch, SHA, status, log, diff, remotes — see the code.
Enter
Sleep 2.5s
Type "j"
Sleep 500ms
Type "j"
Sleep 700ms
Type "b"
Sleep 800ms

# Sort the fleet (caret ▲/▼), then mark repos for a bulk action.
Type ">"
Sleep 900ms
Type ">"
Sleep 1s
Type " "
Sleep 500ms
Type "j"
Type " "
Sleep 1.2s

# Fleet-wide open PR/MRs — github + gitlab, with review + CI state.
Type "m"
Sleep 2s

# Drill into a PR: reviewers, CI checks, body.
Enter
Sleep 2.5s
Type "b"
Sleep 700ms

# Merge it — confirm-gated (reaches the network). Cancel here.
Type "M"
Sleep 2s
Escape
Sleep 800ms
Type "b"
Sleep 700ms

# Recent CI runs, then drill in to jobs + steps.
Type "i"
Sleep 1.5s
Enter
Sleep 2.5s
Type "b"
Sleep 600ms
Type "b"
Sleep 700ms

# Switch theme live — six built-in skins, NO_COLOR aware.
Type ":theme dracula"
Enter
Sleep 1.8s

# Governance — registered plugins, SBOM/provenance, findings.
Type "v"
Sleep 2s
Type "b"
Sleep 700ms

Type "q"
Sleep 1.2s
