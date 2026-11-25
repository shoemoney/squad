# Three reviews said "not buildable." The fourth plan was to stop writing the plan that way.

*2026-11-24. Day one of Squad.*

Squad started the day as a web app and ended it as a terminal app with a different architecture, seven modes instead of one, and a spec that had failed three reviews. This is what changed and what changed it.

## The stack moved four times before noon

| Morning plan | What replaced it | What decided it |
|---|---|---|
| Vue web app plus a terminal client | Terminal only | Nobody should need a browser for this |
| TypeScript server | Rust, one binary | Once the client was Rust, sharing types pointed the server at Rust too |
| Ink for the terminal UI | Ratatui | See below |
| Redis pub/sub | In-process channels and one SQLite file | One binary with nothing to install beat a nice demo |

Ink was the first pick and it was wrong. Ten separate research passes came back, and the pattern in them was hard to argue with: the AI terminal apps that rewrote their UI moved to renderers that repaint only changed cells, and the ones still on Ink were patching flicker around it. The only independent benchmark anyone found had Ratatui delivering 300 of 300 frames at about 0.7 ms each, and Ink delivering 105 of 300 at about 5 ms. One author, one synthetic app, so it is a data point and not a law. It was still the only number in the pile that did not come from the author of the framework being promoted.

Redis went last. It was in the original brief and it made a good story. It also meant every user would have to start a service before the app would open.

## The room designed its own best mode

To test the idea before any of it existed, the same prompt went to all six models through OpenRouter, in parallel, none able to see the others. The prompt described the three modes we had and asked for one new one.

Run one: four of six proposed some way of measuring where the room agrees, rather than another way to pick a winner.

Run two stated the goal outright, a plan where everything has been thought of, and asked what was still missing. **Four of six independently reached for capture-recapture**, the method ecologists use to count animals they cannot see. If six independent searchers keep finding the same gaps in a plan, few are left. If every find is unique, many are. Two of the models gave it the same name, `$census`.

That is now a mode. One of them added the caveat that keeps it honest: models share blind spots, so the estimate is a floor.

Both runs together cost about $0.50.

## The test run also caught a bug in the spec

In run two, one model returned nothing. It had spent its whole 4,000-token allowance reasoning and wrote zero words. With a larger allowance it answered in 4,614 tokens, 319 of them visible words.

An independent review had flagged exactly this an hour earlier as a blocker: the spec capped the judge's per-round scoring at 512 tokens with reasoning on, which would have returned empty scorecards round after round. Reading it in a review is one thing. Watching a real call come back empty is what made the fix non-negotiable. Every limit in the spec is now a limit on visible output, with the reasoning budget counted separately.

The same run showed a model writing 508 words against a stated limit of 350. Word limits get enforced by the app, not requested.

## Three reviews, three "no"s

The spec was drafted by ten writers in parallel, one section each, from a shared brief. That was fast. It also failed review three times, and the third failure is the one worth writing down.

1. **Review one:** three incompatible verdict formats, two config schemas, three shapes for a unit of work.
2. **Review two**, after every writer was given the findings and a table naming the single owner of each shared contract: types referenced but never defined, work scheduled before the milestone that defined its types.
3. **Review three:** "Three sections each say another one owns these types." So they were declared nowhere.

The ownership table had worked exactly as written. Each writer stopped defining what it did not own. Each owner assumed its neighbour's contract was the neighbour's to write. Nobody could see the hand-off had been dropped on both sides. Meanwhile the document grew from 22,000 words to 155,000 across five passes, and every mechanical check passed: unique IDs, no stale terms, clean formatting.

What changed:

- **One author for anything shared.** Fan-out is for the parts that depend on the shared contracts, not for the contracts.
- **A smaller first milestone.** Core messaging and one mode. Each other mode's data shapes get defined right before that mode is built.
- **Compiled types as the contract.** A mismatch should be a compile error, not a paragraph nobody cross-read.

The tell, for next time: the findings get fixed, the checks go green, and the next review still says no with a different list. That is the moment to change who writes the shared parts, not to run the same pass again.

## Building it twice

The command parser was built twice, by two workers, from the same spec. Both passed everything, including all 162 examples in the spec. A reviewer then ran about 175 hostile inputs through both and found 14 places where they behaved differently and 13 places where the spec allowed two readings. Neither build alone would have shown any of that, because each had quietly picked a reading and written tests that agreed with itself.

The winner was merged. The loser had been right on five of the fourteen, and those were ported across.

## Where it stands

Foundations are merged and green on macOS and Linux. The parser is in. A code review of the foundations found a way the API key could be printed in a crash message, which is first on tomorrow's list. Nothing draws on screen yet.
