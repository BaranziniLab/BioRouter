import { describe, expect, it, vi } from 'vitest';
import { validateExternalBrowserTarget } from './externalBrowserNavigation';

describe('validateExternalBrowserTarget', () => {
  it('returns the exact normalized hostname after public-target validation', async () => {
    const validatePublic = vi.fn(async (candidate: string) => new URL(candidate));
    await expect(
      validateExternalBrowserTarget('https://Example.TEST/docs?q=1', validatePublic)
    ).resolves.toEqual(new URL('https://example.test/docs?q=1'));
    expect(validatePublic).toHaveBeenCalledWith('https://example.test/docs?q=1');
  });

  it.each([
    'https://127.0.0.1/',
    'https://203.0.113.8/',
    'https://[::1]/',
    'https://[2606:4700:4700::1111]/',
  ])('refuses literal address %s before public-target validation', async (candidate) => {
    const validatePublic = vi.fn(async (url: string) => new URL(url));
    await expect(validateExternalBrowserTarget(candidate, validatePublic)).rejects.toThrow(
      /Literal IP addresses/
    );
    expect(validatePublic).not.toHaveBeenCalled();
  });

  it('propagates a private DNS resolution refusal', async () => {
    const validatePublic = vi.fn(async () => {
      throw new Error('Local addresses are not available to embedded pages.');
    });
    await expect(
      validateExternalBrowserTarget('https://rebinding.example/', validatePublic)
    ).rejects.toThrow(/Local addresses/);
  });

  it('fails closed if validation does not return the exact requested URL', async () => {
    const validatePublic = vi.fn(async () => new URL('https://other.example/'));
    await expect(
      validateExternalBrowserTarget('https://example.test/', validatePublic)
    ).rejects.toThrow(/changed the requested target/);
  });
});
