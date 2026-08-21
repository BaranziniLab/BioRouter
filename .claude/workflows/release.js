/* global agent, phase, log, args */
// BioRouter release workflow — orchestrates a signed, notarized, multi-platform
// release using agents over scripts/release.sh. Runs in the Workflow sandbox,
// which provides the globals declared above (not standalone Node), so a plain
// ESLint pass would otherwise flag them as no-undef.
//
// Run it with the Workflow tool:
//   Workflow({ name: 'release', args: { version: '1.80.1' } })
// or, ad-hoc:
//   Workflow({ scriptPath: '.claude/workflows/release.js', args: { version: '1.80.1' } })
//
// Each phase delegates to one focused agent that runs a single
// `scripts/release.sh <phase> <version>` step and reports a structured verdict,
// so the workflow can stop early on the first failure and you can resume from
// any phase. The heavy builds are necessarily serial (every bundle rewrites
// ui/desktop/src/bin and the cross builds share the cargo target lock), so the
// package phase runs one platform at a time on purpose.

export const meta = {
  name: 'release',
  description: 'Cut a signed, notarized, multi-platform BioRouter release (bump → build → notarize → publish)',
  whenToUse: 'When you want to ship a new BioRouter version end-to-end. Pass { version: "x.y.z" }.',
  phases: [
    { title: 'Prep', detail: 'bump version + write release notes + commit' },
    { title: 'Backends', detail: 'compile mac arm64/x64 + windows + linux release binaries' },
    { title: 'Package', detail: 'sign+notarize mac dmgs, package windows zip + linux deb/rpm' },
    { title: 'Verify', detail: 'check arch, notarization, asset set, and source provenance' },
    { title: 'Draft', detail: 'push main, prove exact remote equality, then create the 11-asset draft' },
    { title: 'Publish', detail: 'uploaded digests + fresh windows smoke gate, then flip the draft live' },
  ],
}

const VERDICT = {
  type: 'object',
  additionalProperties: false,
  required: ['ok', 'detail'],
  properties: {
    ok: { type: 'boolean', description: 'true only if the step fully succeeded' },
    detail: { type: 'string', description: 'one-paragraph result: artifact paths, sizes, or the error tail' },
  },
}

const version = (args && (args.version || args.v)) || (typeof args === 'string' ? args : null)
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error('release workflow needs a semver version, e.g. Workflow({ name: "release", args: { version: "1.80.1" } })')
}

// Run one release.sh phase in a dedicated agent and fail fast on a bad verdict.
async function step(phase, instructions) {
  const res = await agent(
    `You are running one phase of the BioRouter release for version ${version}, from the repo root ` +
    `/Users/wgu/Desktop/BioRouter. ${instructions}\n\n` +
    `Run the command, stream nothing back except what you need, and return a verdict. ` +
    `Treat a non-zero exit, a missing artifact, "MISSING", "NOT stapled", "WRONG ARCH", or any link/compile error as ok=false. ` +
    `Notarization and docker builds can take 10-20 minutes each — wait for them.`,
    { label: phase, phase: metaTitle(phase), schema: VERDICT },
  )
  if (!res || !res.ok) {
    throw new Error(`[${phase}] failed: ${res ? res.detail : 'agent returned null'}`)
  }
  log(`✓ ${phase}: ${res.detail.slice(0, 200)}`)
  return res
}

function metaTitle(phase) {
  if (phase === 'prep') return 'Prep'
  if (phase === 'backends') return 'Backends'
  if (phase === 'verify') return 'Verify'
  if (phase === 'publish') return 'Publish'
  return 'Package'
}

phase('Prep')
await step('prep',
  `Run \`bash scripts/release.sh bump ${version}\` to bump all 6 version-bearing files in lockstep. ` +
  `Then write concise patch/minor release notes to docs/releases/notes/v${version}.md based on \`git log <previous-tag>..HEAD\` ` +
  `(model the format on the latest existing docs/releases/notes/*.md — Downloads table with the 5 platform files named for ${version}, ` +
  `What's New / What's Fixed, Upgrading, Changes Since). Then commit ONLY the 6 version-bearing files + the new release notes with message ` +
  `"release v${version}". Do not add Co-Authored-By, AI-generated, or other automated attribution trailers; the commit policy rejects them. ` +
  `Do NOT commit unrelated working-tree changes.`)

phase('Backends')
await step('backends',
  `Run \`bash scripts/release.sh backends ${version}\`. This compiles the release backend for mac arm64, mac x64, ` +
  `windows-gnu (docker), and linux-gnu (docker), applying the winpthread + LZMA_API_STATIC cross-compile fixes. ` +
  `Confirm target/release, target/x86_64-apple-darwin/release, target/x86_64-pc-windows-gnu/release (.exe + 3 dlls), ` +
  `and target/x86_64-unknown-linux-gnu/release all hold fresh binaries.`)

// Package phase — STRICTLY serial (each bundle clobbers ui/desktop/src/bin).
// Linux is last because its docker package leaves node_modules Linux-flavored.
phase('Package')
await step('mac-arm64', `Run \`bash scripts/release.sh mac-arm64 ${version}\` — signs + notarizes the Apple Silicon dmg. Verify out/make/BioRouter-${version}-arm64.dmg exists and the app reports "Notarized Developer ID".`)
await step('mac-intel', `Run \`bash scripts/release.sh mac-intel ${version}\` — signs + notarizes the Intel dmg. Verify out/make/BioRouter-${version}-x64.dmg exists and its bundled binary is x86_64.`)
await step('windows',   `Run \`bash scripts/release.sh windows ${version}\` — packages the Windows zip. Verify out/make/zip/win32/x64/Biorouter-win32-x64-${version}.zip exists and contains resources/bin/biorouterd.exe.`)
await step('linux',     `Run \`bash scripts/release.sh linux ${version}\` — packages the Linux GUI deb + rpm via docker (it leaves node_modules Linux-flavored). Verify the .deb and .rpm exist.`)
await step('cli-linux', `Run \`bash scripts/release.sh cli-linux ${version}\` — builds the 2 headless CLI-only Linux packages (biorouter + biorouterd) via docker, smoke-tested in clean Debian/Rocky containers. Verify dist/cli/biorouter-cli_${version}_amd64.deb and dist/cli/biorouter-cli-${version}-1.x86_64.rpm exist. These are 2 of the 11 required release assets.`)

await step('headless-linux', `Run \`bash scripts/release.sh headless-linux ${version}\` — builds the browser-served headless Linux tarball. Verify dist/biorouter-headless-linux-x64.tar.gz exists. Do not use mtime as freshness evidence: release.sh records its digest and source SHA in dist/release-build-${version}.tsv, and verify/draft/publish reject stale or changed bytes. This is 1 of the 11 required release assets.`)

phase('Verify')
await step('verify',
  `First restore a mac-native node_modules: \`cd ui/desktop && rm -rf node_modules && npm ci\` (the linux package corrupted it). ` +
  `Then run \`bash scripts/release.sh verify ${version}\`. It must accept all 11 local assets against the durable ` +
  `dist/release-build-${version}.tsv provenance manifest; a missing manifest, changed source SHA, dirty source tree, stale artifact, ` +
  `digest mismatch, duplicate, or extra asset is a hard failure. ` +
  `The 11 assets are 5 GUI (arm64.dmg, x64.dmg, win32-x64 zip, amd64.deb, x86_64.rpm), 2 headless CLI ` +
  `(biorouter-cli_*_amd64.deb, biorouter-cli-*-1.x86_64.rpm), 1 headless tarball ` +
  `(biorouter-headless-linux-x64.tar.gz) and 3 macOS auto-update artifacts ` +
  `(Biorouter-darwin-arm64-*.zip, Biorouter-darwin-x64-*.zip, latest-mac.yml). Without the last three, ` +
  `macOS clients 404 on the in-app updater and fall back to an assisted download.`)

phase('Draft')
await step('draft',
  `FIRST push the release commits: \`git push origin main\`. This is not optional and not cosmetic — ` +
  `drafting from unpushed commits can tag the previous source while uploading this version's assets. \`cmd_draft\` now requires ` +
  `a successful fetch and exact \`HEAD == origin/main\`, then targets that immutable commit SHA. ` +
  `Then run \`bash scripts/release.sh draft ${version}\`, which regenerates latest-mac.yml, asserts all 11 ` +
  `assets exist, and creates the DRAFT release. It deliberately stops there.`)

phase('Publish')
await step('publish',
  `Publication is gated on a native Windows smoke run, because nothing earlier in this pipeline executes the ` +
  `Windows build on Windows. After the draft upload finishes, trigger the \`release-artifact-smoke.yml\` workflow for v${version} ` +
  `and wait for it to succeed, then run \`bash scripts/release.sh publish ${version}\`. It re-runs verify, compares all 11 ` +
  `uploaded GitHub SHA-256 digests and sizes to the local provenance-bound files, and requires a successful smoke for the same ` +
  `source SHA whose start time is later than the newest draft asset upload. Any replaced asset therefore requires a new smoke run. ` +
  `Finally confirm \`gh release view v${version}\` shows exactly 11 uploaded assets and is not a draft.`)

// (A top-level `return` here is valid inside the Workflow async-wrapper runtime
// but a fatal parse error to a plain ESLint pass — the workflow's completion is
// reported via this log line instead.)
log(`release workflow complete: v${version} released`)
