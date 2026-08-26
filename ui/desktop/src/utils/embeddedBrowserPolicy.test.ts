import { describe, expect, it, vi } from 'vitest';
import dns from 'node:dns/promises';
import {
  assertPublicEmbeddedUrl,
  isAuthenticationNavigation,
  isPrivateNetworkAddress,
  resolvePublicEmbeddedHost,
} from './embeddedBrowserPolicy';

describe('isPrivateNetworkAddress', () => {
  it.each([
    '0.0.0.0',
    '10.0.0.1',
    '127.0.0.2',
    '169.254.169.254',
    '172.31.0.1',
    '192.168.1.1',
    '100.64.0.1',
    '192.0.2.1',
    '198.18.0.1',
    '198.51.100.1',
    '203.0.113.1',
    '255.255.255.255',
    '::1',
    '0:0:0:0:0:0:0:1',
    '0:0:0:0:0:0:0:0',
    'fc00::1',
    'fd00:0:0:0:0:0:0:1',
    'fe80::1',
    'fe80:0:0:0:0:0:0:1',
    'fe80::1%lo0',
    'fec0::1',
    '::ffff:127.0.0.1',
    '::ffff:7f00:1',
    '0:0:0:0:0:ffff:7f00:1',
    '::127.0.0.1',
    '0:0:0:0:0:0:127.0.0.1',
    '::ffff:0:127.0.0.1',
    '::8.8.8.8',
    '64:ff9b::127.0.0.1',
    '64:ff9b:1::1',
    '100::1',
    '2002:7f00:1::',
    '2001:0:0:0:0:0:80ff:fffe',
    '2001:2::1',
    '2001:db8::1',
    '3fff::1',
  ])('blocks %s', (address) => expect(isPrivateNetworkAddress(address)).toBe(true));

  it.each([
    '8.8.8.8',
    '1.1.1.1',
    '100.63.255.255',
    '100.128.0.0',
    '198.17.255.255',
    '198.20.0.0',
    '2606:4700:4700::1111',
  ])('admits %s', (address) => expect(isPrivateNetworkAddress(address)).toBe(false));
});

describe('assertPublicEmbeddedUrl', () => {
  it.each([
    'http://localhost./',
    'http://127.0.0.2/',
    'http://198.18.0.1/',
    'http://192.0.2.1/',
    'ws://[::ffff:7f00:1]/',
    'http://[0:0:0:0:0:0:0:1]/',
    'https://[fec0:0:0:0:0:0:0:1]/',
    'https://[2001:db8::1]/',
  ])('blocks local alias %s', async (url) =>
    expect(assertPublicEmbeddedUrl(url)).rejects.toThrow(/Local addresses/)
  );

  it('blocks a public hostname when DNS resolves it into the LAN', async () => {
    vi.spyOn(dns, 'lookup').mockResolvedValueOnce([
      { address: '192.168.1.50', family: 4 },
    ] as never);
    await expect(assertPublicEmbeddedUrl('https://rebinding.example/')).rejects.toThrow(
      /Local addresses/
    );
  });

  it.each([
    ['198.18.0.1', 4],
    ['203.0.113.10', 4],
    ['fec0::1', 6],
    ['2001:db8::10', 6],
  ])('blocks a hostname whose DNS answer is non-public: %s', async (address, family) => {
    const lookup = vi.fn(async () => [{ address, family }]);
    await expect(resolvePublicEmbeddedHost('non-public.test', lookup)).rejects.toThrow(
      /Local addresses/
    );
  });

  it('returns the exact public address that a caller must use for its socket', async () => {
    const lookup = vi.fn(async () => [{ address: '8.8.8.8', family: 4 }]);
    await expect(resolvePublicEmbeddedHost('example.test', lookup)).resolves.toEqual({
      address: '8.8.8.8',
      family: 4,
    });
    expect(lookup).toHaveBeenCalledOnce();
  });

  it('fails closed when any DNS answer is private', async () => {
    const lookup = vi.fn(async () => [
      { address: '8.8.8.8', family: 4 },
      { address: '127.0.0.1', family: 4 },
    ]);
    await expect(resolvePublicEmbeddedHost('rebinding.test', lookup)).rejects.toThrow(
      /Local addresses/
    );
  });

  const unusableAnswers: Array<{
    label: string;
    answers: Array<{ address: string; family: number }>;
  }> = [
    { label: 'empty response', answers: [] },
    { label: 'malformed address', answers: [{ address: 'not-an-ip', family: 4 }] },
    { label: 'mismatched family', answers: [{ address: '8.8.8.8', family: 6 }] },
  ];
  it.each(unusableAnswers)('fails closed for $label', async ({ answers }) => {
    const lookup = vi.fn(async () => answers);
    await expect(resolvePublicEmbeddedHost('invalid.test', lookup)).rejects.toThrow(
      /Local addresses/
    );
  });
});

describe('isAuthenticationNavigation', () => {
  it.each([
    'https://accounts.example/oauth/authorize?client_id=abc',
    'https://example.test/sign-in',
    'https://login.example.test/',
  ])('recognizes %s', (url) => expect(isAuthenticationNavigation(url)).toBe(true));

  it('does not treat an ordinary article as authentication', () => {
    expect(isAuthenticationNavigation('https://example.test/articles/authorization-study')).toBe(
      false
    );
  });
});
