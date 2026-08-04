# Privacy tiers — what happens to your existing chats

> **What this is.** What the privacy-tiers upgrade does to the chats you already have: which
> ones are marked private, how that decision is made, what it cannot know, and what to do about
> a chat it got wrong.
> **Status:** Current — describes the one-time migration that ships with privacy tiers
> ([issue #56](https://github.com/BaranziniLab/biorouter/issues/56), §15).
> **Audience:** anyone upgrading an existing Biorouter install, and support answering "why is
> this chat suddenly refusing my model?"

## The release note

> *"Chats from before this version are marked by the model they were last using. If an older
> chat contains work you want kept private, switch it to a private model — it will be marked
> private from its next turn on."*

## What the upgrade does

The first launch after upgrading adds three columns to the session database, creates the
declassification ledger, and then runs a **one-time backfill**: every chat whose last bound
model was a private-tier provider — Versa (Azure), Versa (Bedrock), Llama Server or Ollama —
is marked **private**, with a provenance of `backfill:<provider>`.

Everything else is left **public**. That includes chats with no recorded model at all, which on
a real machine is the largest group of the three.

Before enforcement starts affecting your chats, Biorouter shows a one-screen notice with the
**actual numbers from your own database** — how many chats were marked, broken down by the
model that marked them, and how many record no model at all. Those numbers are computed at
that moment; nothing in the product quotes a figure from a design document.

## Why some chats are marked private and others are not

The backfill reads one field: the model a chat was **last** bound to. It does not read a single
message.

That has a consequence worth stating plainly, because you will meet it:

- A chat that ran on Versa and was later switched to a commercial model is marked **public**,
  even though its transcript contains private-model work.
- A chat that ran on a commercial model and was switched to Ollama for one turn is marked
  **private**, even though nothing sensitive is in it.

There is no transcript scan and there will not be one. Scanning message bodies to decide a
privacy tier would mean reading every conversation on the machine to protect the few, and it
would still be a guess.

## Why the unknown cases are left public

A chat with no recorded model could be anything. The migration marks it public.

This is deliberate, and it is the least-bad option rather than a comfortable one. The
alternative — treat "unknown, and it has messages" as private — was rejected because of what it
does to someone who has never used a private model at all: a large slice of their history would
come back private on first launch and refuse the model they normally work with, and the only
way out is declassification, one chat at a time, irreversibly. A wrong guess in that direction
costs the user their history; a wrong guess in this direction leaves a chat exactly as exposed
as it was before the upgrade.

## Fixing a chat the migration got wrong

**A chat that should be private but is not:** open it and switch it to a private model. From
its next turn on it is marked private, and it stays that way.

**A chat that is private and should not be:** declassify it, from the chat's own privacy
control (or `biorouter session declassify <id>` for a chat History does not list). This is
one-way and it is recorded in an append-only ledger. Chats marked by the backfill get the
stronger confirmation, because a `backfill:` tier is an **inference Biorouter made from the
last-used model**, not something you told it — unlike a chat marked because a turn actually ran
against a private endpoint.

## Knowledge bases

Knowledge bases that already exist start **public**, whatever fed them, for the same
fail-open reason as the sessions above: the tree keeps no record of which chat wrote which
page. If you know a base holds private material, mark it private yourself from the Knowledge
view — you do not have to wait for the next private ingest to raise it.

## Running it twice

The backfill runs **once**, from the numbered schema migration, and never again. This matters:
declassifying a chat deliberately leaves its bound model alone (a public chat is allowed to run
a private model), so a backfill that re-ran on every launch would silently re-mark every chat
you had just declassified. That is the one user-only action in the whole feature, and nothing
in the product may undo it on your behalf.

For support: the migration logs its four counts at `info` on the run that performs it —
`backfilled_private`, `backfilled_public_named`, `backfilled_unknown_provider`,
`backfilled_empty` — alongside the three visible counts the notice showed.

## Rolling back

Downgrading leaves the columns in place and ignored, and the ledger inert. Nothing is moved,
re-indexed or rewritten. Rolling forward again re-runs nothing, because the migration is
versioned. The one-way door is the backfill itself.

## Related documentation

- [Privacy tiers](privacy-tiers.md) — the design this migration serves: how models, sessions,
  extensions and knowledge bases acquire a tier, and the enforcement gates. §15 covers the
  migration and §16 the measured cost.
- [Privacy tiers — implementation plan](privacy-tiers-execution-plan.md) — the execution plan,
  including this task's own gates.
- [Data privacy and patient data](data-privacy-and-phi.md) — which providers are acceptable for
  PHI in the first place.
- [Security](README.md) — the rest of the security documentation.
