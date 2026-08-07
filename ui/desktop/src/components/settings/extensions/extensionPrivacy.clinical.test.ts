import { describe, expect, it } from 'vitest';
import type { ExtensionEntry } from '../../../api/types.gen';
import {
  CLINICAL_CREDENTIAL_MARKERS,
  declaresClinicalCredentials,
  publicClinicalExtensions,
} from './extensionPrivacy';

/**
 * §13.5's day-one extension disclosure, and issue #56's Open question 13 — the
 * rule behind the sentence Task 38's first-run notice puts on screen.
 *
 * The three conjuncts are the whole point: **enabled**, **Public**, and
 * **declares clinical-looking credentials**. A private clinical extension is the
 * design working; a disabled one reaches nothing; what the notice exists to
 * surface is the case the design fails open on and no refusal will ever teach —
 * an extension wired to patient data that a commercial model can still call.
 */
function ext(name: string, extra: Record<string, unknown> = {}): ExtensionEntry {
  return { type: 'stdio', name, description: name, enabled: true, ...extra } as ExtensionEntry;
}

describe('declaresClinicalCredentials', () => {
  it('reads the operator machine own case: medcp declares CLINICAL_RECORDS_PASSWORD', () => {
    expect(
      declaresClinicalCredentials(ext('medcp', { env_keys: ['CLINICAL_RECORDS_PASSWORD'] }))
    ).toBe(true);
  });

  it('reads the plaintext half of the declaration too, not only the stored secrets', () => {
    // `medcp` splits its clinical connection across both: the password is an
    // `env_keys` entry (OS credential store) and the server, database and
    // username sit in `envs`. A reader of one half alone still finds `medcp`,
    // which is exactly why it would go unnoticed for a differently-configured
    // sibling that keeps everything in `envs`.
    expect(
      declaresClinicalCredentials(
        ext('sibling', { envs: { CLINICAL_RECORDS_SERVER: 'db.example', LOG_LEVEL: 'info' } })
      )
    ).toBe(true);
  });

  it('is case-insensitive over the marker, not the whole name', () => {
    expect(declaresClinicalCredentials(ext('x', { env_keys: ['my_patient_api_key'] }))).toBe(true);
  });

  it('says nothing about an extension that declares no credentials at all', () => {
    expect(declaresClinicalCredentials(ext('autovisualiser'))).toBe(false);
  });

  it('does not fire on an ordinary token', () => {
    expect(declaresClinicalCredentials(ext('x', { env_keys: ['API_TOKEN', 'BASE_URL'] }))).toBe(
      false
    );
  });

  it('every shipped marker actually matches something', () => {
    // Anti-vacuity: a typo in the list is invisible otherwise — the notice would
    // simply stop naming one class of extension and no test would move.
    for (const marker of CLINICAL_CREDENTIAL_MARKERS) {
      expect(declaresClinicalCredentials(ext('x', { env_keys: [`${marker}_PASSWORD`] }))).toBe(
        true
      );
    }
  });
});

describe('publicClinicalExtensions', () => {
  const entries = [
    ext('medcp', { env_keys: ['CLINICAL_RECORDS_PASSWORD'] }),
    // Private per the compiled-in marketplace set: the design is working, and
    // naming it would tell the user to worry about the one case that is covered.
    ext('ucsfomopagent', { env_keys: ['OMOP_PASSWORD'] }),
    ext('cdwagent', { env_keys: ['CDW_PASSWORD'] }),
    // Public and clinical, but switched off — it reaches nothing.
    ext('dormant', { enabled: false, env_keys: ['PATIENT_TOKEN'] }),
    // Enabled and public, but nothing about it points at patient data.
    ext('autovisualiser'),
  ];

  it('names exactly the enabled, public, clinical-credentialled extensions', () => {
    expect(publicClinicalExtensions(entries)).toEqual(['medcp']);
  });

  it('returns nothing on a machine with none, so the notice hides the paragraph', () => {
    expect(publicClinicalExtensions([ext('autovisualiser')])).toEqual([]);
  });

  it('is stably ordered, so the notice does not reshuffle between renders', () => {
    const many = [
      ext('zeta', { env_keys: ['PHI_KEY'] }),
      ext('alpha', { env_keys: ['EHR_KEY'] }),
      ext('mid', { env_keys: ['MRN_KEY'] }),
    ];
    expect(publicClinicalExtensions(many)).toEqual(['alpha', 'mid', 'zeta']);
    expect(publicClinicalExtensions([...many].reverse())).toEqual(['alpha', 'mid', 'zeta']);
  });
});
