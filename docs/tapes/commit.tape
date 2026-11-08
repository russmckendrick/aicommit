# aic --tui: stage, stream, commit.
# Render with: vhs docs/tapes/commit.tape  (needs a configured provider)
Output docs/tapes/commit.gif
Set Shell "bash"
Set FontSize 15
Set Width 1400
Set Height 820
Set Theme "GitHub Dark"
Set TypingSpeed 60ms

Hide
Type "cd $(sh docs/tapes/demo-repo.sh) && clear"
Enter
Sleep 1s
Show

Type "aic --tui"
Sleep 400ms
Enter
Sleep 6s
Down
Sleep 600ms
Down
Sleep 1s
Type "t"
Sleep 1.2s
Type "t"
Sleep 1.2s
Type "c"
Sleep 3s
