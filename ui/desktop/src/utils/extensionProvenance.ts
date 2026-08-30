// ui/desktop/src/utils/extensionProvenance.ts
//
// Issue #56 Task 43 (DR-23): where an installed extension came from.
//
// DR-23 says an extension's privacy tier is re-derived from the BAAM registry
// at read time, keyed on a stable identifier — never written onto the local
// config entry, which round-trips through user-writable `config.yaml`.
// Measurement said the stable identifier did not exist: a `.brxt` install
// recorded no provenance at all, so the only join available was the accident
// that the registry `id` (`cdwagent`) and the bundle `manifest.name`
// (`CDWAgent`) reduce to the same key. Rename the config entry and the join
// misses — which removed ENFORCEMENT, since Gates C, E, F1 and F2 in the daemon
// all read that answer.
//
// This module writes the missing half. The reader is Rust:
// `crates/biorouter/src/privacy/provenance.rs`, whose `PROVENANCE_FILE`
// constant must equal the one below and whose `name_to_key` must equal
// `nameToKey`.
//
// ⚠ **Why the main process writes it, and why that needs no privilege.** The
// registry id exists only where the marketplace install happens, which is here.
// The daemon's resolver UNIONS the recorded id with the config-name join, so a
// record can only ever RAISE a tier: forging one, corrupting this file, or
// deleting it leaves the answer at least as restrictive as what shipped before
// this task. That is DR-23's own argument — re-deriving removes the problem
// instead of guarding it — and it is why this file has no gated writer while
// the session store does.
//
// ⚠ The config directory is hardcoded to `~/.config/biorouter`, the same way
// the `brxt:install` handler that calls this already hardcodes
// `~/.config/biorouter/extensions`. Rust resolves it through `Paths`, which
// honours the `BIOROUTER_PATH_ROOT` test seam; the two therefore diverge under
// that seam, which is a test-only relocation and not a shipped configuration.

import fsSync from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import * as crypto from 'crypto';

/** Must equal `provenance::PROVENANCE_FILE` in the Rust crate. */
export const PROVENANCE_FILE = 'extension-provenance.json';
export const PROVENANCE_MUTATIONS_DIR = `${PROVENANCE_FILE}.d`;

/** Must equal `provenance::SCHEMA_VERSION` in the Rust crate. */
export const PROVENANCE_SCHEMA_VERSION = 1;

export interface ProvenanceRecord {
  install_id?: string;
  /** The BAAM registry `id` — the stable identifier the tier is keyed on. */
  registry_id: string;
  /**
   * Where the bundle was unpacked. ⚠ **This is the field that survives a
   * rename.** The record's map key is the config entry's name at install time,
   * and renaming that entry in `config.yaml` changes both the key and the
   * entry's `name` — so a key-only lookup misses and the tier drops to public,
   * which is the enforcement failure DR-23 names. It does not change the
   * `--directory` argument the install wrote, because that is where the
   * server's code physically lives.
   *
   * ⚠ Must be an ABSOLUTE path, or at minimum contain a path separator. The
   * Rust reader compares it against the config's whole argument list and
   * ignores any recorded value not shaped like a path, so that a record
   * claiming `"run"` cannot match the `run` in every `uv run --directory …`
   * config at once. A bare word written here is a record the daemon will never
   * match — which reads as no provenance, i.e. as a downgrade.
   */
  install_dir?: string;
  source_url?: string;
  bundle_sha256?: string;
  recorded_at?: string;
}

export interface ProvenanceStore {
  version: number;
  extensions: Record<string, ProvenanceRecord>;
}

/**
 * The extension manager's map key, byte-for-byte what
 * `config::extensions::name_to_key` produces: every whitespace character
 * removed, then lowercased. A record filed under any other spelling is a record
 * the daemon will never find, which reads as "no provenance" — i.e. as a
 * downgrade for a renamed entry.
 */
export function nameToKey(name: string): string {
  return name.replace(/\s/gu, '').toLowerCase();
}

/**
 * Merge one record into whatever was already stored.
 *
 * Anything unparseable starts a fresh store rather than throwing: this runs at
 * the end of a successful install, and failing the install over a corrupt
 * sidecar would be a worse outcome than losing one record. Entries already
 * present are preserved, so a second install never silently orphans the first.
 */
export function mergeProvenance(
  existing: unknown,
  key: string,
  record: ProvenanceRecord
): ProvenanceStore {
  const prior =
    existing &&
    typeof existing === 'object' &&
    typeof (existing as ProvenanceStore).extensions === 'object' &&
    (existing as ProvenanceStore).extensions !== null &&
    !Array.isArray((existing as ProvenanceStore).extensions)
      ? (existing as ProvenanceStore).extensions
      : {};
  return {
    version: PROVENANCE_SCHEMA_VERSION,
    extensions: { ...prior, [key]: record },
  };
}

/** `~/.config/biorouter` — see the file header for why this is hardcoded. */
export function biorouterConfigDir(): string {
  return path.join(os.homedir(), '.config', 'biorouter');
}

export interface RecordProvenanceInput {
  /** The name the config entry is about to be written under (`manifest.name`). */
  extensionName: string;
  /** The BAAM registry `id` this bundle came from. */
  registryId: string;
  /** Where the bundle was unpacked — the rename-proof half of the join. */
  installDir?: string;
  /** The URL it was downloaded from, when there was one. */
  sourceUrl?: string;
  /** The downloaded `.brxt`, hashed as evidence. Missing/unreadable is fine. */
  bundlePath?: string;
  /** Overridable so tests do not write into the real config directory. */
  configDir?: string;
  now?: () => string;
}

/**
 * Record where `extensionName` came from. Returns the record written, or `null`
 * if nothing could be written — the caller treats that as non-fatal, because
 * losing provenance costs the rename protection and nothing else, whereas
 * failing the install costs the user their extension.
 */
export function recordExtensionProvenance(input: RecordProvenanceInput): ProvenanceRecord | null {
  const configDir = input.configDir ?? biorouterConfigDir();
  try {
    let bundleSha256: string | undefined;
    if (input.bundlePath) {
      try {
        bundleSha256 = crypto
          .createHash('sha256')
          .update(fsSync.readFileSync(input.bundlePath))
          .digest('hex');
      } catch {
        // Evidence only; its absence must not stop the record being written.
      }
    }

    const record: ProvenanceRecord = {
      install_id: crypto.randomUUID(),
      registry_id: input.registryId,
      ...(input.installDir ? { install_dir: input.installDir } : {}),
      ...(input.sourceUrl ? { source_url: input.sourceUrl } : {}),
      ...(bundleSha256 ? { bundle_sha256: bundleSha256 } : {}),
      recorded_at: (input.now ?? (() => new Date().toISOString()))(),
    };

    const mutationDir = path.join(configDir, PROVENANCE_MUTATIONS_DIR);
    fsSync.mkdirSync(mutationDir, { recursive: true });
    const key = nameToKey(input.extensionName);
    const mutationId = record.install_id!;
    const target = path.join(mutationDir, `${mutationId}.json`);
    const tmp = path.join(mutationDir, `.${mutationId}.tmp`);
    fsSync.writeFileSync(
      tmp,
      `${JSON.stringify({ op: 'upsert', key, record })}\n`
    );
    fsSync.renameSync(tmp, target);
    const currentDir = path.join(mutationDir, 'current');
    fsSync.mkdirSync(currentDir, { recursive: true });
    const pointerName = Buffer.from(key, 'utf8').toString('hex');
    const pointerTarget = path.join(currentDir, pointerName);
    const pointerTmp = path.join(currentDir, `.${mutationId}.tmp`);
    fsSync.writeFileSync(pointerTmp, `${JSON.stringify({ key, install_id: mutationId })}\n`);
    fsSync.renameSync(pointerTmp, pointerTarget);
    return record;
  } catch {
    return null;
  }
}
