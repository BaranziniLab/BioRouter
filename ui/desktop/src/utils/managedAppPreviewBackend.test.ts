import { EventEmitter } from 'node:events';
import type { BrowserWindow } from 'electron';
import type { BiorouterdResult } from '../biorouterd';
import { describe, expect, it } from 'vitest';
import { bindManagedAppPreviewBackend } from './managedAppPreviewBackend';

function fixture(managed = true, signalCode: string | null = null) {
  const process = Object.assign(new EventEmitter(), {
    exitCode: null as number | null,
    signalCode,
    killed: false,
  });
  const owner = Object.assign(new EventEmitter(), { isDestroyed: () => false });
  const result = {
    baseUrl: 'http://127.0.0.1:64005',
    managed,
    process,
    workingDir: '/tmp/qa',
    errorLog: [],
  } as unknown as BiorouterdResult;
  return {
    process,
    owner,
    result,
    context: bindManagedAppPreviewBackend(result, owner as unknown as BrowserWindow),
  };
}

describe('managed app provenance and lifetime', () => {
  it('never grants an externally configured daemon, even on loopback', () => {
    expect(fixture(false).context).toBeUndefined();
  });
  it.each(['exit', 'error'])('revokes on process %s without waiting for stdio close', (event) => {
    const f = fixture();
    expect(f.context?.signal.aborted).toBe(false);
    f.process.emit(event, event === 'error' ? new Error('synthetic') : 0);
    expect(f.context?.signal.aborted).toBe(true);
    expect(f.process.listenerCount('close')).toBe(0);
  });
  it('revokes only the closing owner, not another owner of the same daemon', () => {
    const f = fixture();
    const other = Object.assign(new EventEmitter(), { isDestroyed: () => false });
    const second = bindManagedAppPreviewBackend(f.result, other as unknown as BrowserWindow)!;
    f.owner.emit('closed');
    expect(f.context?.signal.aborted).toBe(true);
    expect(second.signal.aborted).toBe(false);
    f.process.emit('exit', 0);
    expect(second.signal.aborted).toBe(true);
  });
  it('does not revive a prior generation at the same address', () => {
    const first = fixture();
    first.process.emit('exit', 0);
    const second = fixture();
    expect(first.context?.signal.aborted).toBe(true);
    expect(second.context?.signal.aborted).toBe(false);
    expect(first.context).not.toBe(second.context);
  });
  it('starts revoked when the managed child has already exited', () => {
    const f = fixture();
    f.process.exitCode = 1;
    expect(
      bindManagedAppPreviewBackend(f.result, f.owner as unknown as BrowserWindow)?.signal.aborted
    ).toBe(true);
  });
  it('keeps an observed backend error revoked for later owners', () => {
    const f = fixture();
    f.process.emit('error', new Error('synthetic backend error'));
    const laterOwner = Object.assign(new EventEmitter(), { isDestroyed: () => false });
    expect(
      bindManagedAppPreviewBackend(f.result, laterOwner as unknown as BrowserWindow)?.signal.aborted
    ).toBe(true);
  });
  it('revokes a new owner binding after a signal-only exit, even when close is pending', () => {
    const f = fixture();
    f.process.signalCode = 'SIGKILL';
    f.process.emit('exit', null, 'SIGKILL');
    expect(f.context?.signal.aborted).toBe(true);
    const laterOwner = Object.assign(new EventEmitter(), { isDestroyed: () => false });
    const later = bindManagedAppPreviewBackend(f.result, laterOwner as unknown as BrowserWindow);
    expect(f.process.exitCode).toBeNull();
    expect(f.process.killed).toBe(false);
    expect(later?.signal.aborted).toBe(true);
  });
  it('rejects a process already signal-terminated before its first binding', () => {
    const f = fixture(true, 'SIGTERM');
    expect(f.process.exitCode).toBeNull();
    expect(f.process.killed).toBe(false);
    expect(f.context?.signal.aborted).toBe(true);
  });
});
