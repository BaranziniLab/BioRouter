import net, { type Server, type Socket } from 'node:net';
import { afterEach, describe, expect, it } from 'vitest';
import { startManagedAppPreviewProxy } from './managedAppPreviewProxy';

const cleanups: Array<() => Promise<void>> = [];
const clients = new Set<Socket>();
afterEach(async () => {
  for (const client of clients) client.destroy();
  clients.clear();
  for (const cleanup of cleanups.reverse()) await cleanup();
  cleanups.length = 0;
});

async function listener(
  onConnection: (socket: Socket) => void
): Promise<{ server: Server; port: number }> {
  const sockets = new Set<Socket>();
  const server = net.createServer((socket) => {
    sockets.add(socket);
    socket.on('error', () => socket.destroy());
    socket.on('close', () => sockets.delete(socket));
    onConnection(socket);
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen({ host: '127.0.0.1', port: 0 }, resolve);
  });
  cleanups.push(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });
  return { server, port: (server.address() as net.AddressInfo).port };
}

async function connect(port: number): Promise<Socket> {
  const socket = net.createConnection({ host: '127.0.0.1', port });
  clients.add(socket);
  await new Promise<void>((resolve, reject) => {
    socket.once('connect', resolve);
    socket.once('error', reject);
  });
  return socket;
}

function read(socket: Socket, length: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let result = Buffer.alloc(0);
    const timer = setTimeout(() => done(new Error('Synthetic SOCKS response timed out')), 3000);
    const done = (error?: Error) => {
      clearTimeout(timer);
      socket.removeListener('data', data);
      socket.removeListener('error', done);
      socket.removeListener('end', ended);
      if (error) reject(error);
      else resolve(result);
    };
    const data = (chunk: Buffer) => {
      result = Buffer.concat([result, chunk]);
      if (result.length >= length) done();
    };
    const ended = () => done(new Error('Socket ended before response'));
    socket.on('data', data);
    socket.once('error', done);
    socket.once('end', ended);
  });
}

async function socks(
  port: number,
  host: string,
  destinationPort: number
): Promise<{ socket: Socket; status: number }> {
  const socket = await connect(port);
  const greeting = read(socket, 2);
  socket.write(Buffer.from([5, 1, 0]));
  expect(await greeting).toEqual(Buffer.from([5, 0]));
  const name = Buffer.from(host);
  const request = Buffer.concat([
    Buffer.from([5, 1, 0, 3, name.length]),
    name,
    Buffer.from([destinationPort >> 8, destinationPort & 255]),
  ]);
  const response = read(socket, 10);
  socket.write(request);
  return { socket, status: (await response)[1] };
}

describe('single-destination managed app transport', () => {
  it('connects only the exact daemon socket and tunnels its response', async () => {
    const daemon = await listener((socket) => socket.on('data', () => socket.write('app-ok')));
    const lifetime = new AbortController();
    const proxy = await startManagedAppPreviewProxy(daemon.port, lifetime.signal);
    cleanups.push(proxy.close);
    const { socket, status } = await socks(proxy.port, '127.0.0.1', daemon.port);
    expect(status).toBe(0);
    const reply = read(socket, 6);
    socket.write('GET /apps/qa/ HTTP/1.1\r\n\r\n');
    expect((await reply).toString()).toBe('app-ok');
  });

  it('denies another TCP/TURN port with a live positive-control sentinel', async () => {
    let sentinelConnections = 0;
    const daemon = await listener(() => {});
    const sentinel = await listener((socket) => {
      sentinelConnections += 1;
      socket.once('data', (chunk) => {
        expect(chunk.toString()).toBe('synthetic-positive-control');
        socket.write('observed');
      });
    });
    const control = await connect(sentinel.port);
    const observed = read(control, 8);
    control.write('synthetic-positive-control');
    expect((await observed).toString()).toBe('observed');
    control.destroy();
    expect(sentinelConnections).toBe(1);
    const proxy = await startManagedAppPreviewProxy(daemon.port, new AbortController().signal);
    cleanups.push(proxy.close);
    const blocked = await socks(proxy.port, '127.0.0.1', sentinel.port);
    expect(blocked.status).not.toBe(0);
    expect(sentinelConnections).toBe(1);
  });

  it.each([
    'localhost',
    '127.1',
    '2130706433',
    '127.0.0.2',
    '10.0.0.1',
    '169.254.169.254',
    'example.test',
    '8.8.8.8',
  ])('rejects %s without resolving or connecting', async (host) => {
    let connections = 0;
    const daemon = await listener(() => {
      connections += 1;
    });
    const proxy = await startManagedAppPreviewProxy(daemon.port, new AbortController().signal);
    cleanups.push(proxy.close);
    expect((await socks(proxy.port, host, daemon.port)).status).not.toBe(0);
    expect(connections).toBe(0);
  });

  it('revokes the listener and established tunnel immediately on abort', async () => {
    const daemon = await listener(() => {});
    const lifetime = new AbortController();
    const proxy = await startManagedAppPreviewProxy(daemon.port, lifetime.signal);
    cleanups.push(proxy.close);
    const { socket, status } = await socks(proxy.port, '127.0.0.1', daemon.port);
    expect(status).toBe(0);
    const closed = new Promise<void>((resolve) => socket.once('close', () => resolve()));
    lifetime.abort();
    await closed;
    await expect(connect(proxy.port)).rejects.toBeDefined();
  });

  it('refuses an already-revoked lifetime', async () => {
    const lifetime = new AbortController();
    lifetime.abort();
    await expect(startManagedAppPreviewProxy(64005, lifetime.signal)).rejects.toThrow(
      'unavailable'
    );
  });
});
