import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ParameterInputModal from './ParameterInputModal';

afterEach(cleanup);

describe('ParameterInputModal', () => {
  it('uses Escape to enter and leave the cancel choice without abandoning the workflow', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(
      <ParameterInputModal
        parameters={[
          {
            key: 'topic',
            description: 'Topic',
            input_type: 'string',
            requirement: 'required',
          },
        ]}
        onSubmit={() => {}}
        onClose={onClose}
      />
    );

    expect(screen.getByText('Workflow Parameters')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.getByText('Cancel Workflow Setup')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.getByText('Workflow Parameters')).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
