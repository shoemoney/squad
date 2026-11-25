# Alias resolution cache

Model-using startup stores successful `~vendor/...-latest` resolutions, including
judge aliases, in SQLite's `alias_cache` table (migration `0008_alias_cache.sql`).
Entries are keyed by the configured OpenRouter base URL and exact alias. They
retain validated concrete model capabilities and the successful probe timestamp.
Restarting within the TTL reuses the concrete identity and makes no paid probes;
reading an entry never renews its timestamp. Catalog GETs can still occur.

`SQUAD_ALIAS_CACHE_TTL_SECONDS` configures the TTL in seconds through the normal
environment / `--env-file` loader. The default is **86400 (24 hours)**. Expiry is
inclusive: an entry exactly 86400 seconds old is expired. Clock rollback expires
an entry rather than extending it. Failed resolutions are not cached.

To explicitly refresh all configured aliases on an existing model-using command:

```sh
SQUAD_ALIAS_CACHE_TTL_SECONDS=0 squad send '@claude hello'
```

Use a normal TTL again on subsequent commands. Zero disables cache reuse for
that invocation; successful probes still update the durable cache. `squad models`
remains cache-only, and doctor, model listing, and help make no provider calls,
even with TTL zero.

A missing-model HTTP 404, provider 404, or recognized model-not-found HTTP 400 /
stream error triggers one refresh of affected aliases per rejected concrete slug
per room lifetime. Refresh probes pass through the same durable admission and
billing hooks as startup probes. The failed work retains its frozen identity and
returns its original failure; no paid work is replayed inside the transport. The
next room startup consumes the repaired cache. If repair fails, the invalid entry
has already been removed and the next startup attempts resolution again.

Cached identities remain authoritative for the TTL even if a fresh catalog omits
them. Configured concrete selections still require catalog validation. Changing
an alias or provider endpoint cannot reuse an unrelated entry.
