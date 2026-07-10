export interface WorkflowDeeplinkData {
  config: string;
  parameters?: Record<string, string>;
}

/**
 * Pull the encoded workflow and its parameter overrides out of a
 * `biorouter://workflow?config=…` deep link.
 *
 * The current encoder emits URL-safe base64 without padding, but links minted by
 * older versions use standard base64, whose `+` a URLSearchParams read silently
 * turns into a space. Those links must keep working, hence the raw-query fallback.
 *
 * Returns undefined for anything unusable rather than throwing — callers are
 * event handlers reacting to a URL the user pasted, and a throw there would take
 * down the launch.
 */
export function parseWorkflowDeeplink(url: string): WorkflowDeeplinkData | undefined {
  let parsedUrl: URL;
  try {
    parsedUrl = new URL(url);
  } catch {
    return undefined;
  }

  let workflowDeeplink = parsedUrl.searchParams.get('config');
  const search = parsedUrl.search || '';

  if (workflowDeeplink && !url.includes(workflowDeeplink)) {
    // The decoded value isn't a substring of the URL, so decoding changed it
    // (`+` → space). Re-read the raw query to preserve the original bytes.
    const configMatch = search.match(/(?:[?&])config=([^&]*)/);
    const rawConfig = configMatch ? configMatch[1] : null;
    if (rawConfig) {
      try {
        workflowDeeplink = decodeURIComponent(rawConfig);
      } catch {
        // A stray '%' makes decodeURIComponent throw; the raw value is still the
        // best guess at what the sender meant.
        workflowDeeplink = rawConfig;
      }
    }
  }

  if (!workflowDeeplink) {
    return undefined;
  }

  // Every query param except the two reserved ones is a workflow parameter value.
  const parameters: Record<string, string> = {};
  for (const match of search.matchAll(/[?&]([^=&]+)=([^&]*)/g)) {
    const key = match[1];
    const rawValue = match[2];
    if (key === 'config' || key === 'scheduledJob') {
      continue;
    }
    try {
      parameters[key] = decodeURIComponent(rawValue);
    } catch {
      parameters[key] = rawValue;
    }
  }

  return {
    config: workflowDeeplink,
    parameters: Object.keys(parameters).length > 0 ? parameters : undefined,
  };
}
