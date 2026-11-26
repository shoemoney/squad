| Environment variable | Default | Meaning and validation |
|---|---|---|
| `OPENROUTER_BASE_URL` | `https://openrouter.ai/api/v1/` | Provider API base URL, including trailing slash; loopback fixtures in tests |
| `OPENROUTER_API_KEY` | None | Required nonempty credential for generation and online model checks |
| `SQUAD_ROUNDS` | `10` | Integer from 1 through 20; seeds `ROUNDS` only when no room value is stored |
| `SQUAD_MODEL_<NICK>` | Six defaults below when none are set | One member per variable; valid unique nick and nonempty OpenRouter slug |
| `SQUAD_MODEL_CLAUDE` | `~anthropic/claude-opus-latest` | Default `claude` selection |
| `SQUAD_MODEL_FABLE` | `~anthropic/claude-fable-latest` | Default `fable` selection |
| `SQUAD_MODEL_OPENAI` | `~openai/gpt-sol-latest` | Default `openai` selection |
| `SQUAD_MODEL_GEMINI` | `google/gemini-4-neon` | Default `gemini` selection |
| `SQUAD_MODEL_GROK` | `~x-ai/grok-latest` | Default `grok` selection |
| `SQUAD_MODEL_MUSE` | `meta/muse-wonder` | Default `muse` selection |
| `SQUAD_JUDGE_MODEL` | `~anthropic/claude-fable-latest` | Separate summoned judge model; never creates a member; default selection is independent of custom member selections |
| `SQUAD_USER_NAME` | `the user` | Nonempty text supplied to prompts; the human remains `me` in the interface |
| `SQUAD_PERSONA_<NICK>` | Empty string | Persona for a configured member; appended after the shared room system prompt |
| `SQUAD_DAILY_MAX_USD` | `150` | Positive finite number; the single daily spending max |
| `SQUAD_DEBATE_CONFIRM_USD` | `2` | Nonnegative finite number |
| `SQUAD_ROOM_WORDS` | `500` | Positive integer prompt limit |
| `SQUAD_TURN_WORDS` | `400` | Positive integer prompt limit |
| `SQUAD_FINAL_TURN_WORDS` | `500` | Positive integer prompt limit |
| `SQUAD_REASONING_EFFORT` | `medium` | `low`, `medium`, or `high` for members |
| `SQUAD_JUDGE_REASONING_EFFORT` | `high` | `low`, `medium`, or `high` for final judge calls; `judge.round` uses fixed low effort |
| `SQUAD_JUDGE_FALLBACKS` | `claude,openai` | Ordered fallback member nicks for debate and standalone judge model conflicts; validate member references only when explicitly set |
| `SQUAD_PROMPTS_DIR` | Unset | Use embedded prompts when unset; otherwise validate the selected directory through Prompt pack |
| `SQUAD_DATA_DIR` | OS data directory | Linux: `~/.local/share/squad`; macOS: `~/Library/Application Support/squad` |
| `SQUAD_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error`, or `off` |
| `NO_COLOR` | Unset | A nonempty value disables color |
| `SQUAD_BANNERS` | `on` | `on`, `compact`, or `off`; TUI banner rendering follows TUI |
| `SQUAD_TIPS` | `on` | `on` or `off`; rotate parser-checked composer examples and keyboard tips every 8 seconds while the draft is empty. |
| `SQUAD_FX` | `full` | `full`, `lite`, or `off`; effects behavior follows TUI effects and motion; automatic lite selection applies only when unset |
| `SQUAD_GRILL_ROUNDS` | `2` | Integer from 1 through 10; seeds `GRILL_ROUNDS` only when no room value is stored |
| `SQUAD_PLAN_WORDS` | `1200` | Positive integer prompt limit for rebuilt grill plans |
| `SQUAD_REASONING_EFFORT_<NICK>` | Unset | Optional `low`, `medium`, or `high` override for a configured member |
| `SQUAD_FX_DEBUG` | Unset | `1` shows the effects-grade tag |
