# Renders demo/keel-cli.gif — the colorful CLI walkthrough.
# Regenerate locally with: vhs demo/cli.tape   (needs the release binary built)

Output demo/keel-cli.gif

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

Type "keel sync --stack gateway"
Enter
Sleep 2.5s

Type "keel tree"
Enter
Sleep 2s

Type "echo tweak >> hal/README.md"
Enter
Sleep 500ms

Type "keel status"
Enter
Sleep 2.5s

Type "keel change start FEAT-42 --repos kernel,app-mqtt"
Enter
Sleep 2.5s

Type "keel change status FEAT-42"
Enter
Sleep 3s

Sleep 2s
