# $readback raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Judge finds divergent readings, pauses at READ IT BACK, you choose meanings, spec is rewritten. Human step. About 60 s to the pause, 165 s total.

Output docs/media/full/readback.gif
Output docs/media/full/readback.mp4

Set Shell "bash"
Set FontSize 14
Set Width 1400
Set Height 860
Set Padding 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 45ms
Set WindowBar Colorful

Hide
Type 'unset NO_COLOR; export HOME="$DEMO_HOME" SQUAD_DAILY_MAX_USD=150 SQUAD_DEBATE_CONFIRM_USD=0 && cd "$DEMO_WORK" && clear'
Enter
Sleep 1s
Show

# 1. Open the room
Type "squad"
Sleep 600ms
Enter
Wait+Screen@180s /Ask the squad/
Sleep 2s

# 2. One round, a two-sentence spec with two deliberate ambiguities
Type '@all $readback $READBACK_ROUNDS=1 The service retains records. The operator deletes backups.'
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# 3. Human step: wait for the pause, then pick readings. Entry numbers and letters come from the
#    ledger the judge produced; check the first raw take and adjust (1b 2a assumes two entries).
Wait+Screen@1200s /READ IT BACK/
Sleep 6s
Type "1b 2a"
Sleep 800ms
Enter
Sleep 3s
# If entries remain open, close the step with them unanswered
Type "/continue"
Sleep 500ms
Enter
Wait+Screen@1200s /Complete ·/
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
