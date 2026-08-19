import { describe, expect, it } from 'vitest';
import { looksMachineGenerated, prettyLabel } from './labelText';

describe('looksMachineGenerated', () => {
  it('flags UUIDs and hashes', () => {
    expect(looksMachineGenerated('a64e171e-f161-4615-9299-839c8a066049.pdf')).toBe(true);
    expect(looksMachineGenerated('a64e171e-f161-4615-9299-839c8a066049-pdf-48f040')).toBe(true);
    expect(looksMachineGenerated('deadbeefdeadbeefdeadbeef')).toBe(true);
    expect(looksMachineGenerated('')).toBe(true);
  });

  it('accepts human titles', () => {
    expect(looksMachineGenerated('Effects of e-cigarette aerosol inhalation in mice')).toBe(false);
    expect(looksMachineGenerated('RNA-seq')).toBe(false);
    expect(looksMachineGenerated('Zone-2 base')).toBe(false);
  });
});

describe('prettyLabel', () => {
  it('returns human titles unchanged', () => {
    expect(prettyLabel('Intersubject Variability in Aerosol Deposition')).toBe(
      'Intersubject Variability in Aerosol Deposition'
    );
  });

  it('rescues a machine-generated source label', () => {
    const out = prettyLabel('a64e171e-f161-4615-9299-839c8a066049-pdf-48f040', 'source');
    expect(out).toBe('Untitled source');
  });

  it('strips a trailing short hash from an otherwise readable stem', () => {
    expect(prettyLabel('wanjun-gu---google-scholar-d7f205')).toBe('wanjun-gu---google-scholar');
  });
});
