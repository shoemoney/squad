# $bass_ackwards_prd raw recording (full length, uncut). Record with: docs/demo/new/record-all.sh <squad-binary>
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Acceptance tests, backward prerequisite trees, merged PRD, review, optional human LAST LOOK, saved file. Human step. About 410 to 500 s live.

Output docs/media/full/bass_ackwards_prd.mp4

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

# 2. Turn on the optional human review before starting
Type "/set PRD_HUMAN_REVIEW 1"
Sleep 500ms
Enter
Sleep 1500ms
Type "@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts"
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# 3. Human step: add one note at LAST LOOK, then continue to saving
Wait+Screen@1200s /LAST LOOK/
Sleep 6s
Type "/answer Add an explicit rollback step for the alerting rollout."
Sleep 800ms
Enter
Sleep 2s
Type "/continue"
Sleep 500ms
Enter
# Fixed wait: the report can scroll the status header off screen, so no text marker is reliable
Sleep 150s
# 4. Hold on the saved-file line and coverage summary
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
