# Landing site consolidation — migration record

**Date:** 2026-06-29 · **Status: ✅ COMPLETE — `biorouter.ucsf.edu` now serves from this repo's `landing/`.**

The standalone `BaranziniLab/biorouter-landing` site was vendored into this app
repo under [`landing/`](.) so the website ships and versions with the app and is
easier for the AI agent to maintain. The live custom domain was cut over to be
served from here.

## What was done

1. **Vendored the site** into `landing/` as a clean snapshot (no old git history /
   video blobs imported). Verified **byte-for-byte identical** to the source repo
   except: `.githooks/` dropped (the app repo has its own), and `README.md` /
   `.gitignore` updated for the new location.
2. **`landing/.gitignore` re-includes `*.png`** — the repo-root `.gitignore`
   globally ignores PNGs, but the site ships PNG assets, so they're force-included
   for this subtree.
3. **Added [`.github/workflows/deploy-landing.yml`](../.github/workflows/deploy-landing.yml):**
   on every push to `main` touching `landing/**`, it uploads `landing/` as the
   Pages artifact (served as-is, no Jekyll) and deploys it. `landing/CNAME`
   re-asserts the `biorouter.ucsf.edu` custom domain.
4. **Enabled Pages on `BaranziniLab/biorouter`** with Source = **GitHub Actions**
   (`gh api -X POST repos/BaranziniLab/biorouter/pages -f build_type=workflow`).
5. **Moved the custom domain** `biorouter.ucsf.edu` from `biorouter-landing` to
   `biorouter`: released it from the old repo (`PUT .../biorouter-landing/pages`
   `{"cname": null}`), then claimed it on the new repo
   (`PUT .../biorouter/pages` `{"cname": "biorouter.ucsf.edu"}`). DNS was unchanged
   (it points at the org-level `baranzinilab.github.io`), so GitHub reissued the
   Let's Encrypt cert immediately and the cutover was effectively seamless.

**Verified live:** all pages + assets 200 over HTTPS, valid TLS cert, BAAM
marketplace renders from the relative `registry.json` fetch (6 extensions, 80
skills), zero console errors.

## Current ownership

- **Domain `biorouter.ucsf.edu` → `BaranziniLab/biorouter`** (this repo), served
  from `landing/` via the deploy workflow.
- The old **`biorouter-landing`** repo was **deleted** (remote + local) on
  2026-06-29 after the cutover was verified. Its full 57-commit history was first
  archived to a git bundle (`biorouter-landing-archive-2026-06-29.bundle`) kept
  outside the repo as insurance.

## To update the site from now on

Commit a change under `landing/` to `main` and push this repo; the
`deploy-landing.yml` workflow redeploys Pages within ~1–2 minutes. You can also
trigger it manually from the Actions tab (`workflow_dispatch`).

## Rollback (if ever needed)

The site now lives only in this repo, so rollback means redeploying from here:
re-run the **Deploy landing site** workflow (Actions tab → Run workflow), or
revert the offending `landing/` change and push. The custom domain and Pages
config on this repo are unaffected by content rollbacks.

If the *whole* old repo is ever needed back, restore it from the archived
bundle: `git clone biorouter-landing-archive-2026-06-29.bundle`, then push to a
fresh GitHub repo. (You would only re-add the `biorouter.ucsf.edu` domain to that
repo if you also removed it from this one — a domain lives on one Pages site.)
