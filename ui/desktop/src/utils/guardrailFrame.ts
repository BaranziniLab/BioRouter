/**
 * Hide the tool-output guardrail's internal frame from human-facing surfaces.
 *
 * The backend wraps **every** text block of every successful tool result in an
 * untrusted-data frame before it re-enters the model context
 * (`crates/biorouter/src/guardrails/tool_output.rs`):
 *
 * ```text
 * <tool-output untrusted="true" tool="developer__shell">
 * …the real tool output…
 * </tool-output>
 * ```
 *
 * That frame is a delimiter addressed to the model — `prompts/system.md` names
 * the tag verbatim and tells the model the region is data, never instructions.
 * It is meaningless to the person reading the chat, and worse than meaningless
 * in the panel: `MarkdownContent` runs react-markdown **without `rehype-raw`**,
 * so under CommonMark's HTML-block rule a lone opening tag on its own line
 * swallows the tag *and every line up to the first blank line*. Left alone the
 * frame does not merely show, it eats the first paragraph of every tool result.
 *
 * ## Display only
 *
 * Everything here strips **on the way to a human**. Nothing in this module runs
 * on the path to the model, and the framer is untouched: the control still
 * applies unconditionally, by provenance, exactly as installed. Stripping for
 * the model would be removing the control; stripping for the reader is showing
 * them what the tool actually said.
 *
 * ## The `[BIOROUTER GUARDRAIL]` line is NOT part of the frame
 *
 * A high-confidence injection or PII hit prepends a line **above** the opening
 * tag:
 *
 * ```text
 * [BIOROUTER GUARDRAIL] Tool output flagged: possible prompt-injection markers (…).
 * <tool-output untrusted="true" tool="…">
 * ```
 *
 * That line is a genuine warning to the user and must survive. It survives
 * here structurally rather than by a special case: {@link unwrapGuardrailFrame}
 * only ever rewrites the region **between** a complete opening tag and its
 * matching close. Text outside that region — the warning, any prose the tool
 * emitted before the frame, and every byte of an unframed result — is returned
 * untouched. There is deliberately no "detect the warning" branch to get wrong.
 *
 * ## Why the match is exact-case and non-greedy
 *
 * The framer's `neutralize_frame_close` rewrites any `</tool-output` inside a
 * body to `&lt;/tool-output`, **ASCII-case-insensitively**, so a real body
 * cannot contain the close token in any case. Two consequences:
 *
 * 1. The first `</tool-output>` after an opening tag is always the one the
 *    framer wrote, so a non-greedy body match cannot run past the true close.
 *    A greedy one would, and would swallow every intervening result when a
 *    string carries several frames.
 * 2. The opening tag is matched **case-sensitively**, exactly as emitted. The
 *    framer neutralizes the close token but not the open, so a body can
 *    legitimately contain `<TOOL-OUTPUT untrusted="true" …>` — text the tool
 *    wrote, which the reader is entitled to see. A case-insensitive match
 *    treats that text as a delimiter and deletes it, so the panel stops being
 *    a faithful rendering of what the tool actually said. This is the same
 *    case-fold asymmetry the framer's own test got wrong first time
 *    (`a_close_token_is_neutralized_whatever_its_case_or_trailing_junk`), read
 *    from the other side.
 *
 *    ⚠ Stated precisely because the loose version of this claim survived its
 *    own mutation check: case-insensitivity here is a **fidelity** bug, not a
 *    hiding one. It cannot make a forged tag capture a real frame's close,
 *    because matching is leftmost-first and a real opening tag always precedes
 *    a forged one inside its own body. What it loses is the tag itself, and
 *    with it the reader's only sign that the tool tried to forge a frame.
 *    Nothing between the tags is ever lost; that is guaranteed separately, by
 *    the invariant below.
 *
 * ## Invariant: tags are removed, body text never is
 *
 * Every transform below deletes delimiters and keeps content. That is what
 * makes it safe to run against attacker-influenced text: a tool that forges a
 * frame around a warning cannot use this helper to hide the warning, because
 * unwrapping a frame emits its body.
 */

/** Opening tag up to the tool attribute. Mirrors `TOOL_OUTPUT_FRAME_OPEN`. */
export const GUARDRAIL_FRAME_OPEN = '<tool-output untrusted="true"';

/** Closing tag. Mirrors `TOOL_OUTPUT_FRAME_CLOSE`. */
export const GUARDRAIL_FRAME_CLOSE = '</tool-output>';

/** The close token without its `>`, i.e. what the framer neutralizes. */
const CLOSE_TOKEN = '</tool-output';

/** What the framer rewrites {@link CLOSE_TOKEN} to inside a body. */
const NEUTRALIZED_CLOSE_TOKEN = '&lt;/tool-output';

/**
 * Nesting depth to unwrap before giving up.
 *
 * Frames genuinely nest: a subagent's tool results are concatenated into its
 * final text, which the parent then frames again, so one displayed block can
 * carry a frame per delegation level. The cap exists only so a pathological
 * input cannot spin; hitting it returns the best result so far rather than
 * throwing, because a display helper must never be able to blank a message.
 */
const MAX_UNWRAP_PASSES = 8;

function escapeForRegExp(literal: string): string {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * One complete frame: the opening tag (whatever attributes follow, since the
 * tool name is sanitised to contain no `>`), an optional newline the framer
 * writes, a lazily-matched body, and the closing tag.
 */
const FRAME_PATTERN = new RegExp(
  `${escapeForRegExp(GUARDRAIL_FRAME_OPEN)}[^>]*>\\n?([\\s\\S]*?)\\n?${escapeForRegExp(
    GUARDRAIL_FRAME_CLOSE
  )}`,
  'g'
);

/**
 * Undo the framer's close-token neutralisation, inside a body we have already
 * proved was framed.
 *
 * Scoped to an extracted body on purpose. Applied to arbitrary text it would
 * rewrite an `&lt;/tool-output` a tool emitted on its own; applied to a body
 * the framer just built, the entity is one the framer put there.
 *
 * This is also what makes nesting terminate: an inner frame's close was
 * neutralised when the outer frame was applied, so restoring it turns the
 * inner frame back into something the next pass can match.
 */
function restoreNeutralizedCloseToken(body: string): string {
  if (!body.includes(NEUTRALIZED_CLOSE_TOKEN)) return body;
  return body.split(NEUTRALIZED_CLOSE_TOKEN).join(CLOSE_TOKEN);
}

/**
 * Strip every complete guardrail frame from `text`, returning what the tool
 * actually said.
 *
 * Unframed text is returned **byte for byte**, including the identical string
 * reference, so a session saved before the frame existed renders exactly as it
 * did then. A `[BIOROUTER GUARDRAIL]` warning line above a frame is preserved;
 * only the frame around the body below it is removed.
 */
export function unwrapGuardrailFrame(text: string): string {
  if (!text.includes(GUARDRAIL_FRAME_OPEN)) return text;

  let current = text;
  for (let pass = 0; pass < MAX_UNWRAP_PASSES; pass += 1) {
    const next = current.replace(FRAME_PATTERN, (_match, body: string) =>
      restoreNeutralizedCloseToken(body)
    );
    if (next === current) break;
    current = next;
  }
  return current;
}

/**
 * Apply {@link unwrapGuardrailFrame} to one MCP content block.
 *
 * Returns the **same object** when there was nothing to strip, so unframed
 * content keeps its identity and no consumer re-renders for a no-op. Blocks
 * without a string `text` (images, embedded resources) are never touched: the
 * framer does not frame them either.
 */
export function unwrapGuardrailFrameInContent<T>(item: T): T {
  if (!item || typeof item !== 'object') return item;
  const text = (item as { text?: unknown }).text;
  if (typeof text !== 'string') return item;
  const unwrapped = unwrapGuardrailFrame(text);
  if (unwrapped === text) return item;
  return { ...(item as object), text: unwrapped } as T;
}
