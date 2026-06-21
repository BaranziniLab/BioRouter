// BioRouter release workflow — orchestrates a signed, notarized, multi-platform
// release using agents over scripts/release.sh.
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
    { title: 'Verify', detail: 'check arch, notarization, dmg format, asset set' },
    { title: 'Publish', detail: 'tag, push, gh release create with 5 assets' },
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
  `Run \`bash scripts/release.sh bump ${version}\` to bump the version in all 5 files + Cargo.lock. ` +
  `Then write concise patch/minor release notes to docs/release-notes/v${version}.md based on \`git log <previous-tag>..HEAD\` ` +
  `(model the format on the latest existing docs/release-notes/*.md — Downloads table with the 5 platform files named for ${version}, ` +
  `What's New / What's Fixed, Upgrading, Changes Since). Then commit ONLY the 5 version files + the new release notes with message ` +
  `"release v${version}" and the Co-Authored-By trailer. Do NOT commit unrelated working-tree changes.`)

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
await step('windows',   `Run \`bash scripts/release.sh windows ${version}\` — packages the Windows zip. Verify out/make/zip/win32/x64/BioRouter-win32-x64-${version}.zip exists and contains resources/bin/biorouterd.exe.`)
await step('linux',     `Run \`bash scripts/release.sh linux ${version}\` — packages the Linux GUI deb + rpm via docker (it leaves node_modules Linux-flavored). Verify the .deb and .rpm exist.`)
await step('cli-linux', `Run \`bash scripts/release.sh cli-linux ${version}\` — builds the 2 headless CLI-only Linux packages (biorouter + biorouterd) via docker, smoke-tested in clean Debian/Rocky containers. Verify dist/cli/biorouter-cli_${version}_amd64.deb and dist/cli/biorouter-cli-${version}-1.x86_64.rpm exist. These are 2 of the 7 required release assets.`)

phase('Verify')
await step('verify',
  `First restore a mac-native node_modules: \`cd ui/desktop && rm -rf node_modules && npm install\` (the linux package corrupted it). ` +
  `Then run \`bash scripts/release.sh verify ${version}\`. Also compare the 7 asset names + sizes against the previous GitHub release ` +
  `(\`gh release view <prev-tag> --json assets\`) to confirm the set matches (version-bumped) and nothing is missing. ` +
  `The 7 assets are 5 GUI (arm64.dmg, x64.dmg, win32-x64 zip, amd64.deb, x86_64.rpm) + 2 headless CLI (biorouter-cli_*_amd64.deb, biorouter-cli-*-1.x86_64.rpm).`)

phase('Publish')
await step('publish',
  `Create and push the tag, then publish: \`git push origin main && git tag v${version} && git push origin v${version}\` ` +
  `(skip any that already exist), then \`bash scripts/release.sh publish ${version}\`. ` +
  `Finally confirm \`gh release view v${version}\` shows exactly 7 uploaded assets (5 GUI + 2 CLI) and is not a draft.`)

return { version, status: 'released', release: `v${version}` }
