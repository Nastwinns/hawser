# Elearning: composition lifecycle — how haw materializes a stack of repos.
# Regenerate locally with: vhs demo/cli-compose.tape   (needs the release binary built)

Output demo/cli-compose.gif

Set FontSize 16
Set Width 1100
Set Height 620
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms

Hide
Type `export PATH="$PWD/target/release:$PATH" CLICOLOR_FORCE=1 && . demo/setup.sh`
Enter
Sleep 2s
Show

Type "# haw.toml declares repos + stacks. `tree` shows the intended composition."
Enter
Type "haw tree"
Enter
Sleep 3s

Type "# `sync` clones every repo in a stack and writes haw.lock (reproducible)."
Enter
Type "haw sync --stack gateway"
Enter
Sleep 3s

Type "# `status` is the fleet dashboard: branch / head / dirty / drift per repo."
Enter
Type "haw status"
Enter
Sleep 3s

Type "# `lock` resolves each repo's rev (a branch) to an exact SHA in haw.lock."
Enter
Type "haw lock"
Enter
Sleep 3s

Type "# `pin` snapshots whatever is checked out right now into the lock (no network)."
Enter
Type "haw pin"
Enter
Sleep 3s

Type "# `switch` records another stack as current and materializes just its repos."
Enter
Type "haw switch sensor-node"
Enter
Sleep 3s

Type "# `tree` again — sensor-node is a smaller slice of the same repos."
Enter
Type "haw tree"
Enter
Sleep 3s

Sleep 2s
