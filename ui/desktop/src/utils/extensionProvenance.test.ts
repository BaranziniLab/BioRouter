import fsSync from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  PROVENANCE_MUTATIONS_DIR,
  mergeProvenance,
  nameToKey,
  recordExtensionProvenance,
} from './extensionProvenance';

const dirs: string[] = [];

interface UpsertMutation {
  op: 'upsert';
  key: string;
  record: Record<string, string>;
}

function tempConfigDir(): string {
  const dir = fsSync.mkdtempSync(path.join(os.tmpdir(), 'brxt-prov-'));
  dirs.push(dir);
  return dir;
}

function provenanceMutations(configDir: string): UpsertMutation[] {
  const mutationDir = path.join(configDir, PROVENANCE_MUTATIONS_DIR);
  return fsSync
    .readdirSync(mutationDir)
    .filter((name) => name.endsWith('.json'))
    .map((name) =>
      JSON.parse(fsSync.readFileSync(path.join(mutationDir, name), 'utf8'))
    ) as UpsertMutation[];
}

afterEach(() => {
  while (dirs.length) fsSync.rmSync(dirs.pop()!, { recursive: true, force: true });
});

describe('nameToKey', () => {
  /**
   * Must equal `config::extensions::name_to_key` exactly: every whitespace
   * character removed, then lowercased. A record filed under any other spelling
   * is a record the daemon never finds, which reads as "no provenance" — i.e.
   * as a downgrade for the renamed entry it was written to protect.
   */
  it('matches the Rust reduction the daemon looks records up under', () => {
    expect(nameToKey('CDWAgent')).toBe('cdwagent');
    expect(nameToKey('  UCSF OMOP Agent ')).toBe('ucsfomopagent');
    expect(nameToKey('a__b')).toBe('a__b');
    expect(nameToKey('spokeagent-0.4.1')).toBe('spokeagent-0.4.1');
  });
});

describe('mergeProvenance', () => {
  it('keeps records an earlier install wrote', () => {
    const merged = mergeProvenance(
      { version: 1, extensions: { cdwagent: { registry_id: 'cdwagent' } } },
      'ucsfomopagent',
      { registry_id: 'ucsfomopagent' }
    );
    expect(Object.keys(merged.extensions).sort()).toEqual(['cdwagent', 'ucsfomopagent']);
  });

  /**
   * A corrupt or hostile store must not throw at the end of an otherwise
   * successful install, and must not be carried forward as though it were a
   * map. Both shapes below crashed a first draft that only checked `typeof`.
   */
  it('starts fresh from anything that is not a record map', () => {
    for (const junk of [null, 'nope', 42, [], { extensions: [] }, { extensions: null }]) {
      const merged = mergeProvenance(junk, 'cdwagent', { registry_id: 'cdwagent' });
      expect(merged.extensions).toEqual({ cdwagent: { registry_id: 'cdwagent' } });
      expect(merged.version).toBe(1);
    }
  });
});

describe('recordExtensionProvenance', () => {
  it('writes the shape the Rust reader parses, keyed by the reduced name', () => {
    const configDir = tempConfigDir();
    const bundle = path.join(configDir, 'cdwagent.brxt');
    fsSync.writeFileSync(bundle, 'not really a zip');

    const record = recordExtensionProvenance({
      extensionName: 'CDWAgent',
      registryId: 'cdwagent',
      installDir: '/home/researcher/.config/biorouter/extensions/CDWAgent',
      sourceUrl: 'https://example.test/cdwagent.brxt',
      bundlePath: bundle,
      configDir,
      now: () => '2026-08-03T19:00:00Z',
    });

    expect(record).not.toBeNull();
    const [mutation] = provenanceMutations(configDir);
    expect(mutation.op).toBe('upsert');
    expect(mutation.key).toBe('cdwagent');
    expect(mutation.record.registry_id).toBe('cdwagent');
    expect(mutation.record.install_id).toMatch(/^[0-9a-f-]{36}$/);
    // ⚠ The field the whole feature turns on: renaming the config entry moves
    // the key this record is filed under, so the daemon finds it by the install
    // directory instead. Dropping this makes every rename a downgrade again.
    expect(mutation.record.install_dir).toBe(
      '/home/researcher/.config/biorouter/extensions/CDWAgent'
    );
    expect(mutation.record.source_url).toBe('https://example.test/cdwagent.brxt');
    expect(mutation.record.recorded_at).toBe('2026-08-03T19:00:00Z');
    // sha256 of "not really a zip", so the field is evidence rather than a
    // constant the writer made up.
    expect(mutation.record.bundle_sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  /**
   * ⚠ The record is keyed on the CONFIG name, not on the registry id, and the
   * two diverge for exactly the entry this feature exists for: the config entry
   * is written from `manifest.name`, and `spokeagent-0.4.1` already shows the
   * registry `id` and the installed name disagreeing in the real catalogue.
   */
  it('keys on the config name even when it differs from the registry id', () => {
    const configDir = tempConfigDir();
    recordExtensionProvenance({
      extensionName: 'My Renamed Connector',
      registryId: 'cdwagent',
      configDir,
      now: () => 'now',
    });
    const [mutation] = provenanceMutations(configDir);
    expect(mutation.key).toBe('myrenamedconnector');
    expect(mutation.record.registry_id).toBe('cdwagent');
  });

  it('records even when the bundle cannot be hashed', () => {
    const configDir = tempConfigDir();
    const record = recordExtensionProvenance({
      extensionName: 'cdwagent',
      registryId: 'cdwagent',
      bundlePath: path.join(configDir, 'gone.brxt'),
      configDir,
      now: () => 'now',
    });
    expect(record?.registry_id).toBe('cdwagent');
    expect(record?.bundle_sha256).toBeUndefined();
  });

  it('writes immutable install mutations so concurrent records cannot overwrite each other', () => {
    const configDir = tempConfigDir();
    for (const extensionName of ['first agent', 'second agent']) {
      recordExtensionProvenance({
        extensionName,
        registryId: extensionName.replace(' ', '-'),
        configDir,
        now: () => 'now',
      });
    }

    const mutations = provenanceMutations(configDir);
    expect(mutations).toHaveLength(2);
    expect(mutations.map((mutation) => mutation.key).sort()).toEqual(['firstagent', 'secondagent']);
    expect(new Set(mutations.map((mutation) => mutation.record.install_id)).size).toBe(2);
  });

  it('reports failure rather than throwing when the store cannot be written', () => {
    // A file where the config directory should be: `mkdirSync` fails, so the
    // whole write fails. The install must survive this, not abort on it.
    const parent = tempConfigDir();
    const notADir = path.join(parent, 'config');
    fsSync.writeFileSync(notADir, 'in the way');
    expect(
      recordExtensionProvenance({
        extensionName: 'cdwagent',
        registryId: 'cdwagent',
        configDir: notADir,
      })
    ).toBeNull();
  });
});
