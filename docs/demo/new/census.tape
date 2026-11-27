# $census raw recording (full length, uncut): one pass, a short deploy plan, hold on the report.
# Runs the v0.2.0 release binary (`squad` on PATH). HOME, SQUAD_DATA_DIR and DEMO_WORK are
# private temp dirs made by record-all.sh; the API key comes from the environment and is never typed.
# Dated probability, spread of the room, cruxes and signposts. About 260 s live.

Output docs/media/full/census.gif
Output docs/media/full/census.mp4

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

# 2. One pass only (stored value; the room default is 2), then a short deploy plan
Type "/set CENSUS_PASSES 1"
Sleep 500ms
Enter
Sleep 1500ms
Type "@all $census Deploy plan for v2.4.0: 1) tag v2.4.0 on main, 2) run the database migration on the primary, 3) roll the API out in two batches, 4) turn on the new checkout flow for 10% of users, 5) watch the error rate for 30 minutes, 6) ramp checkout to 100%."
Sleep 800ms
Escape
Sleep 300ms
Enter
Wait+Screen@180s /Confirm/
Sleep 1s
# Preflight card: confirm
Type "y"
# Fixed wait: the report can scroll the status header off screen, so no text marker is reliable
Sleep 300s
# 3. Hold on the census report and estimate
Sleep 10s

# Done: leave the room
Ctrl+C
Sleep 1s
