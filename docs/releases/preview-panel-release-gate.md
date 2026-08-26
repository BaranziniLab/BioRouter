# Preview panel release gate

> **What this is.** The release gates the file preview panel must pass before a build is signed.
> **Status:** Current.
> **Audience:** Whoever is cutting a release.

The preview panel has two different execution paths and both are release
requirements. Passing one is not evidence for the other.

| Surface                                        | Automated gate                                                                 | What it proves                                                                                                                                      |
| ---------------------------------------------- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Images, PDF, DOCX, XLSX, PPTX, annotation crop | `just preview-panel-e2e`; Frontend / Preview panel (Electron) on pull requests | The shipping React components render real fixtures in a native Electron renderer and the capture overlay reports the selected compositor rectangle. |
| Live websites                                  | Manual dev **and packaged-app** gate below                                     | The app main process creates a real `WebContentsView`, navigation remains interactive, and text/capture context reaches the chat.                   |

The file harness intentionally stubs the Electron IPC boundary. It must never be
extended with an iframe or jsdom page and then described as live-website
coverage. Website previews are top-level browsing contexts owned by Electron's
main process; only the real app can exercise that architecture.

## Automated file gate

Run from the repository root:

```sh
just preview-panel-e2e
```

The command must exit zero, list all six coverage labels (`image`, `pdf`,
`docx`, `xlsx`, `pptx`, `annotation-crop`), and emit an evidence directory with
one screenshot per format. The pull-request job uploads the same screenshots as
`preview-panel-evidence-*`.

## Native live-website gate

Run this once with the dev app and once with the exact signed/notarized app that
will be published. Use an isolated `BIOROUTER_PATH_ROOT`, set
`BIOROUTER_DISABLE_KEYRING=true`, and enable the app's CDP port. For the dev app,
`just agent-browser-ui PORT=9333` is the supported launcher. For a packaged app,
set `ENABLE_PLAYWRIGHT=true` and `PLAYWRIGHT_CDP_PORT=9333` when launching its
executable. Do not enter passwords or accept an OAuth/Keychain prompt as part of
this check.

Use a public, non-sensitive test page. `https://example.com/` is sufficient for
the navigation checks; use a stable public form only when form interaction is
being verified, and enter dummy data.

The gate passes only when all of the following are observed:

1. Open the URL from a chat result into the sidebar. The rendered page is live,
   not a snapshot: its link can be followed, scrolling works, and Back, Forward,
   Reload, and direct URL entry update both the pixels and the displayed URL.
2. Resize and move the preview panel. The website follows its bounds without
   covering chat chrome, disappearing, or leaving a stale surface behind.
3. While the site is open, inspect `http://127.0.0.1:9333/json/list`. The website
   URL must appear as a separate Electron target. It must not appear as an iframe
   inside the Biorouter renderer. Closing the preview removes that target.
4. Select a region, add an annotation, and attach it to the composer. The crop
   must contain the website pixels inside the selected bounds, and closing the
   preview must not invalidate the already attached image.
5. Ask the configured agent one question about visible page text and one about
   the selected image region. Both answers must use the current page after
   navigation, not the initial URL or stale pixels.
6. Open the same URL from two sessions, then close each session/tab in turn.
   Each preview remains owned by its session and no orphan website target remains
   after both are closed.
7. Save a full-window screenshot, the annotated-crop screenshot, the redacted
   CDP target list, and the app console/error log with the release evidence.

Any OAuth, password, consent, Keychain, crash, renderer error, stale navigation,
or orphan target is a failed gate. Do not publish until it is resolved and the
dev and packaged runs both pass again.

## Related documentation

- [`docs/desktop-ui/artifact-display-surfaces.md`](../desktop-ui/artifact-display-surfaces.md) — why the panel is the only surface an artifact is displayed on.
- [`docs/deployment/browser-access.md`](../deployment/browser-access.md) — the `biorouter serve` path, whose preview endpoints are a separate execution path from Electron's.
- [`RELEASE.md`](../../RELEASE.md) — the surrounding release process.
