# CodeGraphAgent

This folder holds the design and the two implementation plans for **CodeGraphAgent**, a BioRouter `.brxt` extension that gives agents a pre-indexed code knowledge graph — a persistent SQLite-backed call graph with typed query tools (`codegraph_search`, `codegraph_callers`, `codegraph_impact`, …) built on a Python MCP proxy shim in front of a vendored fork of the [CodeGraph](https://github.com/colbymchenry/codegraph) engine. **This work happened and it shipped.** The design was written 2026-05-29 and revised 2026-05-30; both plans were executed, and CodeGraphAgent is now a live marketplace extension covering 23 languages including the four bioinformatics languages — R, Julia, MATLAB and Perl — that these documents exist to add. Everything here is a historical record, kept to explain *why* the extension is built the way it is. None of it is maintained against the shipped code.

Two boundaries matter before you read further. First, **the code is not in this repository** — CodeGraphAgent lives in [`Broccolito/CodeGraphAgent`](https://github.com/Broccolito/CodeGraphAgent), and that repository is authoritative for the current state of every version number, file path and count named below. These documents are filed under BioRouter's docs only because BioRouter is the consuming application: the `.brxt` format, the install location and the default-off bundling are BioRouter-side concerns. Second, **this is not how to use the extension.** If you want to install, enable or configure CodeGraphAgent — or any other `.brxt` — you are in the wrong folder; go to the [extensions and skills guide](../../extensions/extensions-and-skills-guide.md). Come here only to trace a design decision back to its origin. The unticked `- [ ]` checkboxes throughout both plans are the plans as authored, not outstanding work.

## Documents

| Document | What it covers |
| --- | --- |
| [CodeGraphAgent BioRouter extension design](extension-design.md) | The full design for the extension — the Python MCP proxy shim, the vendored engine fork, the `.brxt` bundle structure, the release pipeline, error handling, testing strategy and the default-off bundling mechanism. The spec both plans implement. |
| [CodeGraphAgent foundation plan](foundation-plan.md) | "Plan 1": the task-by-task plan that scaffolded the repository, built the Python shim and the release pipeline, and shipped `codegraphagent.brxt v0.1.0-rc1` with engine bundles downloaded from GitHub Releases. Its closing section records the finished state, verified end-to-end against a real BioRouter session. |
| [CodeGraphAgent bio-language extractors plan](bio-language-extractors-plan.md) | "Plan 2": the task-by-task plan that added R, Julia, MATLAB and Perl tree-sitter extractors to the vendored engine — including the content heuristic that disambiguates `.m` between MATLAB and Objective-C — and shipped them as `engine-v0.2.0` plus a `.brxt v0.1.0` release. |

Read them in that order if you are new to the extension: the design states the constraints, Plan 1 builds the scaffold, Plan 2 adds the languages on top. Both plans use lettered **phases** containing numbered **tasks** (`Task A1`, `Task R3`, `Task Pe5`); each plan's own header explains its scheme, and the letters carry no meaning beyond the order the phases appear in. Both plans also hardcode the original author's checkout path, `/Users/wgu/Desktop/CodeGraphAgent/` — read it as "your CodeGraphAgent checkout".

## Related documentation

- [Extensions and skills guide](../../extensions/extensions-and-skills-guide.md) — how BioRouter actually installs, enables and configures the `.brxt` these documents produce; the current-guidance counterpart to this folder.
- [Extension manager](../../extensions/built-in/extension-manager.md) — the built-in MCP server that manages extension lifecycle at runtime, and so the component CodeGraphAgent is loaded by.
- [Bundled skills in `.brxt` design](../skills-packaging/brxt-bundled-skills-design.md) — the neighbouring historical design for shipping skills inside a `.brxt`, sharing this folder's packaging format.
- [Historical records index](../README.md) — the rest of BioRouter's completed and abandoned work, if CodeGraphAgent is not what you were looking for.
