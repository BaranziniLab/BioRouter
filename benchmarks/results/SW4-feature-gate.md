# SW4 — Feature-gate heavy dependencies (AWS implemented; pattern for the rest)

**Implemented:** the AWS SDK (Bedrock + SageMaker providers) is now behind a
default-on `aws-providers` feature on the `biorouter` crate. Default builds are
unchanged (feature on → all providers present); `--no-default-features` drops it.

## Measured reduction (AWS, `cargo tree -p biorouter`)
| build | resolved crates in biorouter graph |
|---|---:|
| default (aws-providers on) | 795 |
| `--no-default-features` (aws off) | **753** |
| **dropped** | **42** |

42 crates fall away — AWS pulls `aws-config`, `aws-sdk-bedrockruntime`,
`aws-sdk-sagemakerruntime`, `aws-smithy-*`, `aws-runtime`, plus the
`aws-lc-rs`/`aws-lc-sys` crypto stack and their transitive deps. That is ~5% of
the whole 988-crate workspace from one provider family, and it removes a C/asm
build (`aws-lc-sys`) from the minimal compile.

## Where the win lands
The shipped GUI keeps AWS (no functionality change). The reducible footprint is
realized by builds that opt out — most naturally the **headless CLI-only Linux
packages** (`scripts/build-cli-linux-packages.sh`), which don't need Bedrock:
they would compile 42 fewer crates (incl. the aws-lc C build) and produce a
smaller binary. Wiring those packages to `--no-default-features` (forwarding to
`biorouter`) is the follow-up that turns this into a shipped saving.

## Gating surface (complete, minimal)
`crates/biorouter/Cargo.toml` (4 aws deps → `optional`, `aws-providers` feature)
+ `#[cfg(feature = "aws-providers")]` on: `providers/mod.rs` (bedrock,
sagemaker_tgi, versa_bedrock), `providers/formats/mod.rs` (bedrock), and
`providers/factory.rs` (the 3 imports + 3 registrations). `name_builder.rs` uses
"bedrock" only as a string, so it needs no gate.

## The other three groups (same pattern, not yet applied)
The crate-build analysis flagged three more reducible groups; each follows the
identical optional-dep + `#[cfg]` pattern. Estimated reducible crates (from
`cargo tree`, biorouter-mcp):
- **tree-sitter** (15 grammar crates: python/rust/js/go/java/kotlin/swift/ruby/
  cpp/c/r/julia/matlab + core) → a `code-analysis` feature on `biorouter-mcp`.
- **doc-conversion** (`lopdf`, `pdf-extract`, `docx-rs`, `calamine`,
  `umya-spreadsheet`, `htmd`) → a `doc-conversion` feature.
- **boa_engine** (7 crates, a full JS interpreter) → a `js-engine` feature.

These are deeper (more call sites: the Developer MCP code-intelligence + the
knowledge convert/ pipeline) and riskier to gate completely, so they are left as
documented follow-ups with AWS as the proven template. Doing all four would take
the minimal build well below ~900 crates.

## Verdict
**Real, measured compile/binary win** for opt-out builds (−42 crates from one
family), zero change to the default shipped app. The headless CLI package is the
immediate beneficiary.
