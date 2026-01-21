export const getApiUrl = (endpoint: string): string => {
  const biorouterApiHost = String(window.appConfig.get('BIOROUTER_API_HOST') || '');
  const cleanEndpoint = endpoint.startsWith('/') ? endpoint : `/${endpoint}`;
  return `${biorouterApiHost}${cleanEndpoint}`;
};
