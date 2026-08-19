import log from 'electron-log';
import path from 'node:path';
import { app } from 'electron';

log.transports.file.resolvePathFn = () => {
  if (!app) return path.join(process.env.HOME ?? '/tmp', '.biorouter', 'logs', 'main.log');
  return path.join(app.getPath('userData'), 'logs', 'main.log');
};

log.transports.file.level = app?.isPackaged ? 'info' : 'debug';
log.transports.console.level = app?.isPackaged ? false : 'debug';

// electron-log's file transport defaults to `sync: true`, i.e. one blocking
// `fs.writeFileSync` per line on whichever thread logged it. In the main process
// that is the thread running the window, and the noisy paths are exactly the ones
// that run at startup — a single update check emits ~180 lines (#88). Buffered
// async writes keep the event loop free.
log.transports.file.sync = false;

// The tradeoff, stated plainly: async writes go through a queue that drains on
// the event loop, so a HARD crash (SIGKILL, a native crash) can lose the last few
// lines. A normal quit does not — the fatal-startup-error path in main.ts was
// measured reaching disk. Blocking the window on every one of ~180 lines per
// update check to protect against the hard-crash case is the worse trade.
//
// A log line is never worth blocking on, but it is also never worth crashing on:
// a full disk or a read-only userData dir would otherwise surface as an
// unhandled rejection from a fire-and-forget write.
log.errorHandler?.startCatching?.({ showDialog: false });

export default log;
