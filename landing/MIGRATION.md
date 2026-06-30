# Landing site consolidation — migration & cutover runbook

**Date:** 2026-06-29
**What:** The standalone `BaranziniLab/biorouter-landing` site was vendored into
this app repo under [`landing/`](.) so the website ships and versions with the
app and is easier for the AI agent to maintain.

## What was done (already committed on a branch)

- Copied the full landing site into `landing/` as a clean snapshot (no old git
  history / video blobs imported). Verified **byte-for-byte identical** to the
  source repo except: `.githooks/` dropped (the app repo has its own), and
  `README.md` / `.gitignore` updated for the new location.
- `landing/.gitignore` re-includes `*.png` (the repo-root `.gitignore` globally
  ignores PNGs; the site ships PNG assets, so they're force-included here).
- Added [`.github/workflows/deploy-landing.yml`](../.github/workflows/deploy-landing.yml):
  on every push to `main` touching `landing/**`, it uploads `landing/` as the
  Pages artifact (served as-is, no Jekyll) and deploys it. `landing/CNAME`
  re-asserts the `biorouter.ucsf.edu` custom domain on each deploy.
- Updated `CLAUDE.md` to point at the new in-repo location.

## What was NOT done (the live cutover — your call)

The live site at **https://biorouter.ucsf.edu/** is still served by the old
`biorouter-landing` repo and is **completely untouched**. Nothing was pushed and
no GitHub/DNS settings were changed. The domain flip is below.

## Cutover steps (do these when ready)

A custom domain can be attached to only **one** GitHub Pages site at a time, so
the order matters:

1. **Merge & push this branch to `main`** (gets `landing/` + the workflow onto
   the app repo). The deploy workflow will run but the `deploy` step fails until
   step 3 — that's expected and harmless.
2. **Release the domain from the old repo:**
   `BaranziniLab/biorouter-landing` → Settings → Pages → remove the
   `biorouter.ucsf.edu` custom domain (and optionally set Pages source to
   "None"). This frees the domain.
3. **Enable Pages on this repo:** `BaranziniLab/biorouter` → Settings → Pages →
   Source = **GitHub Actions**.
4. **Re-run the workflow:** Actions tab → "Deploy landing site" → Run workflow
   (or push any `landing/**` change). It deploys and sets `biorouter.ucsf.edu`
   from the bundled CNAME.
5. **Verify** (DNS for the apex/subdomain is unchanged, so propagation is fast):
   - `curl -sI https://biorouter.ucsf.edu/ | head` → `200`
   - `curl -s https://biorouter.ucsf.edu/registry.json | head` → JSON
   - Open the site; confirm BAAM marketplace populates and downloads work.

> Equivalent `gh` commands for steps 2–3 (need repo admin on BaranziniLab):
> ```bash
> # free the domain on the old repo
> gh api -X DELETE repos/BaranziniLab/biorouter-landing/pages 2>/dev/null || true
> # enable Actions-sourced Pages on the app repo
> gh api -X POST repos/BaranziniLab/biorouter/pages -f build_type=workflow
> ```

## Rollback

If anything goes wrong, the old repo still has all files and history. Re-add the
`biorouter.ucsf.edu` custom domain in `biorouter-landing` → Settings → Pages and
it serves again within ~1 minute. Then disable this repo's Pages.

## Deleting the old repo

Only after the cutover is verified working. The old repo is the original and its
git history; deletion is irreversible. Keep it (archived) unless you specifically
want it gone.
