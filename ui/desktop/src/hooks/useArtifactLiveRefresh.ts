import { useEffect, useMemo, useRef, useState } from 'react';
import type { Message } from '../api';
import type { ArtifactSource } from '../components/artifacts/artifactTypes';
import {
  artifactRefreshEvents,
  artifactRefreshTarget,
  refreshEventMatches,
} from '../utils/artifactRefresh';

export function useArtifactLiveRefresh(
  sessionId: string,
  messages: readonly Message[],
  artifact: ArtifactSource | null,
  workingDir: string | undefined,
  ready: boolean
): number {
  const target = artifactRefreshTarget(artifact);
  const scope = JSON.stringify([sessionId, target, ready]);
  const events = useMemo(
    () => (ready && target ? artifactRefreshEvents(messages, sessionId, workingDir) : []),
    [messages, ready, sessionId, target, workingDir]
  );
  const tracker = useRef({
    scope: '',
    seen: new Set<string>(),
    timer: undefined as ReturnType<typeof setTimeout> | undefined,
  });
  const [revision, setRevision] = useState({ scope: '', value: 0 });

  useEffect(() => {
    const current = tracker.current;
    if (current.scope !== scope) {
      clearTimeout(current.timer);
      current.scope = scope;
      current.seen = new Set(events.map((event) => event.id));
      return;
    }
    let changed = false;
    for (const event of events) {
      if (current.seen.has(event.id)) continue;
      current.seen.add(event.id);
      if (ready && target && refreshEventMatches(event, target)) changed = true;
    }
    if (!changed) return;
    clearTimeout(current.timer);
    current.timer = setTimeout(() => {
      if (tracker.current.scope !== scope) return;
      setRevision((previous) => ({
        scope,
        value: previous.scope === scope ? previous.value + 1 : 1,
      }));
    }, 250);
  }, [events, ready, scope, target]);

  useEffect(() => () => clearTimeout(tracker.current.timer), []);
  return revision.scope === scope ? revision.value : 0;
}
