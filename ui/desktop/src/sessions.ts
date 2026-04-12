import { Session, startAgent, ExtensionConfig } from './api';
import type { setViewType } from './hooks/useNavigation';
import {
  getExtensionConfigsWithOverrides,
  clearExtensionOverrides,
  hasExtensionOverrides,
} from './store/extensionOverrides';
import type { FixedExtensionEntry } from './components/ConfigContext';

export function resumeSession(session: Session, setView: setViewType) {
  setView('pair', {
    disableAnimation: true,
    resumeSessionId: session.id,
  });
}

export async function createSession(
  workingDir: string,
  options?: {
    workflowId?: string;
    workflowDeeplink?: string;
    extensionConfigs?: ExtensionConfig[];
    allExtensions?: FixedExtensionEntry[];
  }
): Promise<Session> {
  const body: {
    working_dir: string;
    workflow_id?: string;
    workflow_deeplink?: string;
    extension_overrides?: ExtensionConfig[];
  } = {
    working_dir: workingDir,
  };

  if (options?.workflowId) {
    body.workflow_id = options.workflowId;
  } else if (options?.workflowDeeplink) {
    body.workflow_deeplink = options.workflowDeeplink;
  }

  if (options?.extensionConfigs && options.extensionConfigs.length > 0) {
    body.extension_overrides = options.extensionConfigs;
  } else if (options?.allExtensions) {
    const extensionConfigs = getExtensionConfigsWithOverrides(options.allExtensions);
    if (extensionConfigs.length > 0) {
      body.extension_overrides = extensionConfigs;
    }
    if (hasExtensionOverrides()) {
      clearExtensionOverrides();
    }
  }

  const newAgent = await startAgent({
    body,
    throwOnError: true,
  });

  return newAgent.data;
}

export async function startNewSession(
  workingDir: string,
  initialText: string | undefined,
  setView: setViewType,
  options?: {
    workflowId?: string;
    workflowDeeplink?: string;
    allExtensions?: FixedExtensionEntry[];
  }
): Promise<Session> {
  const session = await createSession(workingDir, options);

  setView('pair', {
    disableAnimation: true,
    initialMessage: initialText,
    resumeSessionId: session.id,
  });

  return session;
}
