# Renderer testing traps

> **What this is.** Ways a renderer test can pass while the code it covers is broken, each
> found by a real defect that a green suite failed to catch. Read it before trusting a
> passing frontend test as evidence.
> **Status:** Current.
> **Audience:** anyone writing or reviewing tests under `ui/desktop/src/`.

The frontend suite is large and fast, which makes it tempting to treat green as proof. These
are the cases where it is not. Each entry names the defect that produced it, so the trap is
concrete rather than theoretical.

## A `vi.fn` spy makes a floating promise unobservable

**The trap.** A component calls an async dependency and does not await or catch it:

```ts
void refreshConfig();   // rejects on a failed re-read; nothing handles it
```

A test that mocks `refreshConfig` with `vi.fn()` **cannot** detect this. `vi.fn` attaches its
own handler to the promise it returns — that is how it records settled results — which marks
the rejection as handled, so `process.on('unhandledRejection')` never fires.

Proven with a control in the same run: a plain `void Promise.reject(new Error('control'))`
was captured by the handler, while the rejection travelling through the spy was not. The
handler reported exactly one rejection, and it was the control.

**Why it matters.** Any test that mocks a promise-returning dependency with `vi.fn` is
structurally incapable of catching a floating-promise defect *at the call site*. This is not a
weak assertion that could be strengthened; the observation channel is closed.

**What to do instead.** Hand the component a plain function rather than a spy when the thing
under test is the call site's error handling. Give it a **stable module-scope identity** — a
fresh closure per render re-fires mount effects and produces spurious "called 2 times,
expected 1" failures that look like a component bug.

**Found by:** a `void refreshConfig()` introduced while fixing the stale config cache. The
first red test passed against the broken code. The same commit had already patched the
identical pattern inside its own test probe, which is what made the gap visible.

## `throwOnError` is off by default in the generated client

**The trap.** The generated API client resolves rather than throws on an HTTP error, so this
looks like sound error handling and is not:

```ts
const response = await readAllConfig();
setConfig(response.data?.config || {});   // a 500 lands here, not in the catch
```

An HTTP 500 **resolves** with `data` absent. The `catch` block is never entered, and the
fallback silently replaces real state with an empty object.

**What to do instead.** Pass `{ throwOnError: true }` when a failed request must not be
mistaken for an empty result, and treat a missing body as a failure of its own. Then decide
explicitly what a failure should leave behind — usually the previous value, not a default.

**Found by:** a config-cache refresh that erased the cache it was written to keep fresh.

## "Newest issued" is the wrong rule for a request-generation counter

**The trap.** The obvious way to make only the latest async read win:

```ts
const gen = ++counter;
const data = await read();
if (gen !== counter) return;   // discard: something newer was issued
```

This discards a result whenever *any* later request has been issued — including one that has
not completed and may fail. The state then keeps whatever it had, which after an initial load
can be nothing at all. The guard against staleness becomes a second way to end up empty.

**What to do instead.** Compare against what was last **applied**, not what was last
**issued**: a result publishes if its ticket is at least the ticket that last published. A
slower newer request still wins when it arrives; a failed newer request cannot suppress an
older successful one.

**Found by:** the first attempt at ordering the config reads. The naive version was written,
tested, and failed — which is the only reason it is not in the tree.

## Related documentation

- [Launching the dev GUI from a shell without a TTY](launching-the-dev-gui.md) — five ways the launcher makes a working app look broken
- [Debugging the dev GUI with agent-browser](agent-browser-debugging.md) — driving the app over CDP
- [Documentation style guide](../contributing/documentation-style.md) — the house style for this tree
