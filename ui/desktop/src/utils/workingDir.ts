export const getInitialWorkingDir = (): string => {
  return (window.appConfig?.get('BIOROUTER_WORKING_DIR') as string) || '';
};
