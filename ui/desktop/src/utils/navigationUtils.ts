import { NavigateFunction } from 'react-router-dom';
import { Workflow } from '../api/types.gen';
import type { UserAttachment } from '../types/message';

export type View =
  | 'welcome'
  | 'chat'
  | 'pair'
  | 'settings'
  | 'extensions'
  | 'moreModels'
  | 'configureProviders'
  | 'configPage'
  | 'ConfigureProviders'
  | 'settingsV2'
  | 'sessions'
  | 'schedules'
  | 'sharedSession'
  | 'loading'
  | 'workflows'
  | 'skills'
  | 'permission';

export type ViewOptions = {
  showEnvVars?: boolean;
  deepLinkConfig?: unknown;
  sessionDetails?: unknown;
  error?: string;
  baseUrl?: string;
  workflow?: Workflow;
  parentView?: View;
  parentViewOptions?: ViewOptions;
  disableAnimation?: boolean;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
  shareToken?: string;
  resumeSessionId?: string;
  pendingScheduleDeepLink?: string;
};

type ViewTransitionDocument = Document & {
  startViewTransition?: (callback: () => void) => { finished: Promise<void> };
};

const prefersReducedMotion = () =>
  typeof window !== 'undefined' &&
  typeof window.matchMedia === 'function' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

export const navigateWithViewTransition = (
  navigate: NavigateFunction,
  to: string,
  state?: unknown,
  options: { replace?: boolean } = {}
) => {
  const runNavigation = () => navigate(to, { replace: options.replace, state });
  const doc = typeof document === 'undefined' ? undefined : (document as ViewTransitionDocument);
  const transition = doc?.startViewTransition;
  const disableAnimation =
    typeof state === 'object' &&
    state !== null &&
    'disableAnimation' in state &&
    Boolean((state as { disableAnimation?: unknown }).disableAnimation);

  if (disableAnimation || prefersReducedMotion() || !transition) {
    runNavigation();
    return;
  }

  transition.call(doc, runNavigation);
};

export const createNavigationHandler = (navigate: NavigateFunction) => {
  return (view: View, options?: ViewOptions) => {
    switch (view) {
      case 'chat':
        navigateWithViewTransition(navigate, '/', options);
        break;
      case 'pair':
        navigateWithViewTransition(navigate, '/pair', options);
        break;
      case 'settings':
        navigateWithViewTransition(navigate, '/settings', options);
        break;
      case 'sessions':
        navigateWithViewTransition(navigate, '/sessions', options);
        break;
      case 'schedules':
        navigateWithViewTransition(navigate, '/schedules', options);
        break;
      case 'workflows':
        navigateWithViewTransition(navigate, '/workflows', options);
        break;
      case 'skills':
        navigateWithViewTransition(navigate, '/skills', options);
        break;
      case 'permission':
        navigateWithViewTransition(navigate, '/permission', options);
        break;
      case 'ConfigureProviders':
        navigateWithViewTransition(navigate, '/configure-providers', options);
        break;
      case 'sharedSession':
        navigateWithViewTransition(navigate, '/shared-session', options);
        break;

      case 'welcome':
        navigateWithViewTransition(navigate, '/welcome', options);
        break;
      case 'extensions':
        navigateWithViewTransition(navigate, '/extensions', options);
        break;
      default:
        navigateWithViewTransition(navigate, '/', options);
    }
  };
};
