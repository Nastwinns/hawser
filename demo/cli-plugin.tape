# Renders demo/haw-plugin.gif — the haw-commit-ai plugin in action.
# Regenerate locally with: vhs demo/cli-plugin.tape   (needs the release binary built)

Output demo/haw-plugin.gif

Set FontSize 18
Set Width 1100
Set Height 640
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 70ms

Hide
Type `export PATH="$PWD/target/release:$PWD/examples/plugins/haw-commit-ai:$PATH" CLICOLOR_FORCE=1 && . demo/setup.sh`
Enter
Sleep 2s
Type `haw sync --stack gateway >/dev/null 2>&1 && printf '\n# new feature flag\n' >> kernel/README.md && printf '\n# wire it through\n' >> app-mqtt/README.md`
Enter
Sleep 1s
Type "clear"
Enter
Show

Sleep 800ms

Type "# A plugin is just a haw-<name> executable on your PATH."
Enter
Sleep 1s
Type "haw plugins list"
Sleep 400ms
Enter
Sleep 3s

Type "# haw commit-ai reads the fleet context and drafts commit + PR text."
Enter
Sleep 1s
Type "haw commit-ai"
Sleep 400ms
Enter
Sleep 4s

Type "# Machine-readable too:"
Enter
Sleep 800ms
Type "haw commit-ai --format json"
Sleep 400ms
Enter
Sleep 3s
