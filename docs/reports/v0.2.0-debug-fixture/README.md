# Controlled Debug smoke fixture

This isolated program deliberately treats numeric zero as a missing rate.
The live models receive its black-box interface and the initial observed batch,
with two competing explanations: default fallback versus cross-item state.
Each probe is a fresh, read-only process. Squad itself executes no commands;
the operator runs the selected probe and submits its actual stdout.

The contract defaults only a missing rate to 0.10 and preserves zero. For
`initial-input.json`, the expected result is `[110.0, 100.0]`. The actual
initial output is retained in `initial-output.json`.

`probe-1.json` through `probe-3.json` preserve the selected instructions,
label maps, actual process argv and stdout for the three `/answer` steps.
`summary.json` records the completed isolated outcome, source, preflight,
provider-call costs and successful offline export. All seven failed transport
attempts settled; final held funds and unknown calls are zero. The illustrative
patch in the final report was not applied to this intentionally faulty fixture.
