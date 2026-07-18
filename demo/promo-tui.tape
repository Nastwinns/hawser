# Renders demo/promo-tui.mp4 — REAL k9s-style cockpit navigation, live.
# Driven by the built-in demo controller: no workspace, no git, no network.
# Regenerate: vhs demo/promo-tui.tape   (needs the release binary built)

Output demo/promo-tui.mp4

Set FontSize 22
Set Width 1280
Set Height 720
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 60ms

Hide
Type `export PATH="$PWD/target/release:$PATH"`
Enter
Type "clear"
Enter
Show

Sleep 800ms

Type "haw dash --demo"
Sleep 300ms
Enter
Sleep 2.5s

# Walk the fleet grid — clean, dirty, drift, a not-cloned repo.
Type "j"
Sleep 900ms
Type "j"
Sleep 1s

# Drill into a repo: branch, SHA, status, log, diff.
Enter
Sleep 2.5s
Type "j"
Sleep 700ms
Type "b"
Sleep 1.2s
Escape
Sleep 700ms

# Fleet-wide open PR/MRs — github + gitlab, review + CI state.
Type "m"
Sleep 2.5s
Enter
Sleep 2.5s
Escape
Sleep 600ms

# Problems-only view — drift + failing CI at a glance.
Type "!"
Sleep 2.5s

Sleep 1s
