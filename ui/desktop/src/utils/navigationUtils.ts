import { NavigateFunction } from 'react-router-dom';
import { Workflow } from '../api/types.gen';

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
  shareToken?: string;
  resumeSessionId?: string;
  pendingScheduleDeepLink?: string;
};

export const createNavigationHandler = (navigate: NavigateFunction) => {
  return (view: View, options?: ViewOptions) => {
    switch (view) {
      case 'chat':
        navigate('/', { state: options });
        break;
      case 'pair':
        navigate('/pair', { state: options });
        break;
      case 'settings':
        navigate('/settings', { state: options });
        break;
      case 'sessions':
        navigate('/sessions', { state: options });
        break;
      case 'schedules':
        navigate('/schedules', { state: options });
        break;
      case 'workflows':
        navigate('/workflows', { state: options });
        break;
      case 'skills':
        navigate('/skills', { state: options });
        break;
      case 'permission':
        navigate('/permission', { state: options });
        break;
      case 'ConfigureProviders':
        navigate('/configure-providers', { state: options });
        break;
      case 'sharedSession':
        navigate('/shared-session', { state: options });
        break;

      case 'welcome':
        navigate('/welcome', { state: options });
        break;
      case 'extensions':
        navigate('/extensions', { state: options });
        break;
      default:
        navigate('/', { state: options });
    }
  };
};
