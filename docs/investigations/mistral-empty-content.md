# Mistral Large 4 empty content investigation

Checked 2026-11-26 through OpenRouter with `mistralai/mistral-large-4-0`.
Five completion calls cost **$0.025640944** in reported usage. No original
conversation or credentials were used. The synthetic input was roughly 700
words requesting a roughly 900-word recommendation about operating a chat room.
Raw responses were inspected before scrubbing the regression fixtures.

| Request | Provider | Max tokens | Reasoning | Visible characters | Finish | Completion / reasoning tokens | Cost USD |
| --- | --- | ---: | --- | ---: | --- | --- | ---: |
| JSON | Mistral | 4000 | medium | 4417 | length | 4000 / 3087 | 0.008989400 |
| JSON | Mistral | 4000 | omitted | 8950 | stop | 1974 / 132 | 0.004853464 |
| SSE | Mistral | 4000 | medium | 7957 | stop | 3980 / 2350 | 0.008624840 |
| JSON, pinned provider | Mistral | 512 | medium | 0 | length | 512 / 511 | 0.001747900 |
| SSE, pinned provider | Mistral | 512 | medium | 0 | length | 512 / 511 | 0.001425340 |

The endpoints API listed only Mistral (standard and ZDR endpoints). The last two
calls explicitly used `provider: {order: ["Mistral"], allow_fallbacks: false}`.
There was no different provider to compare.

The empty JSON response had `choices[0].message.content: null`, `refusal: null`,
and thinking text in `message.reasoning` and `message.reasoning_details` (a
`reasoning.text` item). No tool calls or content array held an answer. The SSE
response likewise carried thinking in `choices[0].delta.reasoning` and
`delta.reasoning_details`, empty `delta.content`, and terminal `length` choices,
including the final usage-bearing choice. Both consumed 511 reasoning tokens of
512 completion tokens, leaving no visible answer. The two original 4000/6000
failures were not reconstructed: these probes demonstrate reasoning budget
exhaustion at a smaller ceiling, with variable reasoning consumption at 4000.
Thinking text must not be substituted for the requested answer.

Before the fix, the transport emitted `Complete` after any finish plus `[DONE]`,
including empty `stop` or `length` output; the ordinary member path could commit
an empty successful reply. The parser now requires non-whitespace answer text,
classifies empty output as reasoning-exhausted truncation, other truncation,
reasoning-only, refusal, unsupported tool
call, provider error, or no content, and reports a safe explanation. Typed
`text` / `output_text` content parts are extracted in order; thinking parts stay
private. Existing nonempty finish handling is unchanged.

Validation happens at `[DONE]`, after terminal usage. Failed billed attempts
remain accounted and reconciled. Ordinary replies retain the existing two-attempt
limit and durable attempt authorization; empty/truncated/reasoning-only failures
are retryable, while refusals and tool calls do not trigger automatic retries.
Engine-owned single-attempt calls remain single-attempt calls.

The wiremock fixtures `mistral-empty.json` and `mistral-empty.sse` preserve the
captured empty shapes and usage, replace generation IDs and thinking text, and
omit repetitive SSE thinking chunks. Regression tests cover those fixtures,
content-part extraction and private thinking exclusion, classification, terminal
usage before failure, ordinary retry exhaustion, visible failure explanation,
and durable failed billing records for both attempts.

Automatic retries keep the frozen request, output ceiling, and reasoning settings.
They do not silently enlarge the visible allowance or lower effort: changing the
allowance requires new admission/reservation sizing, and changing reasoning
would change the frozen work binding. For repeated reasoning exhaustion, newly
authorized work with a larger reserved total ceiling or lower supported effort
is an appropriate manual recovery. Ordinary retries remain bounded to two billed
attempts; judge/contest work remains subject to its existing owner policy.

Final validation on dev3 used `CARGO_BUILD_JOBS=4`:
`cargo test --workspace --locked` (1247 passed, 1 ignored),
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo fmt --all -- --check` all passed.
