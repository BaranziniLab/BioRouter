import net, { type Socket } from 'node:net';

const HANDSHAKE_LIMIT = 4096;
const CONNECT_TIMEOUT_MS = 15_000;

/** A transport boundary, including proxied WebRTC/TURN: no DNS, one socket destination. */
export async function startManagedAppPreviewProxy(
  daemonPort: number,
  signal: AbortSignal
): Promise<{ port: number; close: () => Promise<void> }> {
  if (!Number.isInteger(daemonPort) || daemonPort < 1 || daemonPort > 65535 || signal.aborted) {
    throw new Error('Managed app backend is unavailable.');
  }
  const sockets = new Set<Socket>();
  let stopped = false;
  const server = net.createServer((client) => {
    if (stopped) return void client.destroy();
    sockets.add(client);
    let upstream: Socket | null = null;
    let buffer = Buffer.alloc(0);
    let phase: 'greeting' | 'request' | 'connecting' = 'greeting';
    client.setTimeout(CONNECT_TIMEOUT_MS, () => client.destroy());
    client.on('error', () => client.destroy());
    client.on('close', () => {
      sockets.delete(client);
      upstream?.destroy();
    });
    const reply = (status: number) =>
      client.write(Buffer.from([5, status, 0, 1, 0, 0, 0, 0, 0, 0]));
    const deny = () => {
      reply(2);
      client.end();
    };
    const onData = (chunk: Buffer) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (buffer.length > HANDSHAKE_LIMIT) return deny();
      if (phase === 'greeting') {
        if (buffer.length < 2 || buffer.length < 2 + buffer[1]) return;
        const end = 2 + buffer[1];
        if (buffer[0] !== 5 || !buffer.subarray(2, end).includes(0)) return deny();
        buffer = buffer.subarray(end);
        phase = 'request';
        client.write(Buffer.from([5, 0]));
      }
      if (phase !== 'request' || buffer.length < 4) return;
      if (buffer[0] !== 5 || buffer[1] !== 1 || buffer[2] !== 0) return deny();
      const type = buffer[3];
      if (type !== 1 && type !== 3) return deny();
      if (type === 3 && buffer.length < 5) return;
      const end = type === 1 ? 10 : 7 + buffer[4];
      if (buffer.length < end) return;
      const host =
        type === 1
          ? `${buffer[4]}.${buffer[5]}.${buffer[6]}.${buffer[7]}`
          : buffer.subarray(5, end - 2).toString('utf8');
      if (host !== '127.0.0.1' || buffer.readUInt16BE(end - 2) !== daemonPort) return deny();
      if (stopped || signal.aborted) return deny();
      buffer = buffer.subarray(end);
      phase = 'connecting';
      upstream = net.createConnection({ host: '127.0.0.1', port: daemonPort, family: 4 });
      const target = upstream;
      sockets.add(target);
      target.setTimeout(CONNECT_TIMEOUT_MS, () => target.destroy());
      target.on('error', () => client.destroy());
      target.on('close', () => {
        sockets.delete(target);
        client.destroy();
      });
      target.once('connect', () => {
        if (stopped || signal.aborted || client.destroyed) return void target.destroy();
        client.removeListener('data', onData);
        client.setTimeout(0);
        target.setTimeout(0);
        reply(0);
        if (buffer.length) target.write(buffer);
        client.pipe(target);
        target.pipe(client);
      });
    };
    client.on('data', onData);
  });
  let closing: Promise<void> | null = null;
  const close = () => {
    if (closing) return closing;
    stopped = true;
    for (const socket of sockets) socket.destroy();
    sockets.clear();
    closing = new Promise<void>((resolve) => server.close(() => resolve()));
    return closing;
  };
  const abort = () => void close();
  signal.addEventListener('abort', abort, { once: true });
  server.on('error', abort);
  try {
    await new Promise<void>((resolve, reject) => {
      const aborted = () => failed(new Error('Managed app backend is unavailable.'));
      const failed = (error: Error) => {
        signal.removeEventListener('abort', aborted);
        server.removeListener('error', failed);
        reject(error);
      };
      signal.addEventListener('abort', aborted, { once: true });
      server.once('error', failed);
      server.listen({ host: '127.0.0.1', port: 0, exclusive: true }, () => {
        signal.removeEventListener('abort', aborted);
        server.removeListener('error', failed);
        resolve();
      });
    });
    if (signal.aborted || stopped) throw new Error('Managed app backend is unavailable.');
    server.unref();
    const address = server.address();
    if (!address || typeof address === 'string') throw new Error('Managed app proxy did not bind.');
    return {
      port: address.port,
      close: async () => {
        signal.removeEventListener('abort', abort);
        await close();
      },
    };
  } catch (error) {
    signal.removeEventListener('abort', abort);
    await close();
    throw error;
  }
}
