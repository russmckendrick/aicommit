# aic --tui: split one staged change set into commits, hunk by hunk.
# Render with: vhs docs/tapes/split.tape  (needs a configured provider)
Output docs/tapes/split.gif
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
Sleep 4s
Type "s"
Sleep 7s
Down
Sleep 800ms
Right
Sleep 3s
Enter
Sleep 4s
