export type TurnErrorScope = 'provider' | 'transport' | 'session' | 'inference' | 'internal';

export interface ChatTurnErrorData {
  message: string;
  code: string;
  scope: TurnErrorScope;
  retryable: boolean;
  providerKind?: string;
  technicalDetails?: string;
}
