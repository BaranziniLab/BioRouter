import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChatTurnError, presentChatTurnError } from './ChatTurnError';
import type { ChatTurnErrorData } from '../../types/turnError';

function error(overrides: Partial<ChatTurnErrorData> = {}): ChatTurnErrorData {
  return {
    message: 'Provider rejected the request',
    code: 'provider_failure',
    scope: 'provider',
    retryable: false,
    ...overrides,
  };
}

describe('ChatTurnError', () => {
  it('presents known provider categories without depending on raw provider wording', () => {
    expect(presentChatTurnError(error({ providerKind: 'quota' }))).toMatchObject({
      title: 'Model quota exceeded',
      message: 'Provider rejected the request',
    });
    expect(presentChatTurnError(error({ providerKind: 'auth' }))).toMatchObject({
      title: 'Model authentication failed',
    });
  });

  it('renders unknown future errors with a safe generic fallback', () => {
    render(
      <ChatTurnError
        error={error({
          providerKind: 'brand_new_rejection',
          message: 'A future provider-specific rejection',
        })}
      />
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('Model request failed');
    expect(alert).toHaveTextContent('A future provider-specific rejection');
  });

  it('keeps long technical details wrapped in an expandable region', () => {
    const details = `provider_failure: ${'very-long-provider-payload'.repeat(20)}`;
    render(
      <ChatTurnError
        error={error({
          providerKind: 'unknown',
          technicalDetails: details,
        })}
      />
    );

    expect(screen.getByText('Technical details')).toBeInTheDocument();
    const detailBody = screen.getByText(details);
    expect(detailBody).toHaveClass('break-words');
    expect(detailBody).toHaveClass('[overflow-wrap:anywhere]');
  });
});
