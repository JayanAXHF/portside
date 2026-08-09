# Betamax tape for portside — a deliberately unhurried walkthrough of the core workflow:
# starting a topic, pausing/resuming, taking a break, the session drawer, and the history pane.
# Render with: betamax run demo.tape

Output demo.gif

Require portside

Set Shell "zsh"
Set FontSize 18
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 80ms
Set WaitTimeout 5s

Type "portside"
Enter
Sleep 1.5s

# Start a session — the clock and timer come alive.
Type ":topic writing docs"
Enter
Sleep 3s
Escape

# Pause, sit on it for a beat, then resume.
Type "p"
Sleep 1.5s
Type "r"

Sleep 3s

# Take a break, wait it out, then resume.
Type "b"
Sleep 2.5s
Type "b"
Sleep 2.5s

# Peek at the session drawer.
Tab
Sleep 2.5s
Escape
Sleep 1s

# Browse the history pane across all three views.
Type ":history"
Enter
Sleep 2s
Type "w"
Sleep 2s
Type "c"
Sleep 2s
Type "d"
Sleep 2s
Escape
Sleep 1s

# Wrap the session up and quit.
Type ":complete"
Enter
Sleep 2s

Type "q"
Sleep 1s
