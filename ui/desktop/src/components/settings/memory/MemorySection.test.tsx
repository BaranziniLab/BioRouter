import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import MemorySection from './MemorySection';

const mockInventory = vi.fn();
const mockDeleteEntry = vi.fn();
const mockDeleteCategory = vi.fn();

vi.mock('../../../api', () => ({
  memoryInventory: (...args: unknown[]) => mockInventory(...args),
  memoryDeleteEntry: (...args: unknown[]) => mockDeleteEntry(...args),
  memoryDeleteCategory: (...args: unknown[]) => mockDeleteCategory(...args),
}));

const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();
vi.mock('../../../toasts', () => ({
  toastSuccess: (...args: unknown[]) => mockToastSuccess(...args),
  toastError: (...args: unknown[]) => mockToastError(...args),
}));

vi.mock('../../../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/Users/someone/work/proj',
}));

const entry = (index: number, content: string, tags: string[] = []) => ({
  index,
  tags,
  content,
});

const inventoryResponse = (overrides: Record<string, unknown> = {}) => ({
  data: {
    global: {
      scope: 'global',
      path: '/Users/someone/.config/biorouter/memory',
      exists: true,
      categories: [
        {
          name: 'clinical',
          entries: [
            entry(0, 'the cohort has 812 patients', ['phi', 'cohort']),
            entry(1, 'exclusions are documented in the protocol'),
          ],
          size_bytes: 412,
          modified: 1_780_000_000,
        },
      ],
    },
    local: {
      scope: 'local',
      path: '/Users/someone/work/proj/.biorouter/memory',
      exists: true,
      categories: [
        {
          name: 'development',
          entries: [entry(0, 'we format with black')],
          size_bytes: 24,
          modified: 1_780_000_000,
        },
      ],
    },
    ...overrides,
  },
});

beforeEach(() => {
  vi.clearAllMocks();
  mockInventory.mockResolvedValue(inventoryResponse());
  mockDeleteEntry.mockResolvedValue({ data: { remaining: 1, category_removed: false } });
  mockDeleteCategory.mockResolvedValue({ data: { removed_entries: 2 } });
});

describe('MemorySection', () => {
  it('shows what has been remembered in both stores, with its provenance', async () => {
    render(<MemorySection />);

    // The machine-wide store, which is the one the #63 consent gate asks about.
    expect(await screen.findByText('clinical')).toBeTruthy();
    expect(screen.getByText('/Users/someone/.config/biorouter/memory')).toBeTruthy();

    // Contents are visible, not just category names.
    fireEvent.click(screen.getByRole('button', { name: /show the 2 memories in clinical/i }));
    expect(await screen.findByText('the cohort has 812 patients')).toBeTruthy();
    expect(screen.getByText('exclusions are documented in the protocol')).toBeTruthy();
    // Tags are the only per-entry provenance the store carries.
    expect(screen.getByText('phi')).toBeTruthy();
    expect(screen.getByText('cohort')).toBeTruthy();

    // The project store is listed separately, and named.
    expect(screen.getByText('development')).toBeTruthy();
    expect(screen.getByText('/Users/someone/work/proj/.biorouter/memory')).toBeTruthy();
  });

  it('passes the window project so the local store is this project’s', async () => {
    render(<MemorySection />);
    await screen.findByText('clinical');
    expect(mockInventory).toHaveBeenCalledWith(
      expect.objectContaining({ query: { working_dir: '/Users/someone/work/proj' } })
    );
  });

  it('says the category is machine-wide, how many memories go, and that it cannot be undone', async () => {
    render(<MemorySection />);
    await screen.findByText('clinical');

    fireEvent.click(screen.getByRole('button', { name: /delete the global category clinical/i }));

    const dialog = await screen.findByRole('dialog');
    const text = dialog.textContent ?? '';
    expect(text).toContain('clinical');
    expect(text).toContain('2 memories');
    expect(text.toLowerCase()).toContain('every conversation');
    expect(text.toLowerCase()).toContain('cannot be undone');
    // Nothing is deleted by opening the confirmation.
    expect(mockDeleteCategory).not.toHaveBeenCalled();
  });

  it('deletes a category only after the confirmation is accepted', async () => {
    render(<MemorySection />);
    await screen.findByText('clinical');

    fireEvent.click(screen.getByRole('button', { name: /delete the global category clinical/i }));
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() =>
      expect(mockDeleteCategory).toHaveBeenCalledWith(
        expect.objectContaining({
          body: expect.objectContaining({ scope: 'global', category: 'clinical' }),
        })
      )
    );
  });

  it('quotes the memory it is about to delete, and sends the guard back with it', async () => {
    render(<MemorySection />);
    await screen.findByText('clinical');
    fireEvent.click(screen.getByRole('button', { name: /show the 2 memories in clinical/i }));

    fireEvent.click(
      (await screen.findAllByRole('button', { name: /delete this memory from clinical/i }))[0]
    );

    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('the cohort has 812 patients');
    expect((dialog.textContent ?? '').toLowerCase()).toContain('cannot be undone');

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await waitFor(() =>
      expect(mockDeleteEntry).toHaveBeenCalledWith(
        expect.objectContaining({
          body: {
            scope: 'global',
            category: 'clinical',
            index: 0,
            content: 'the cohort has 812 patients',
            working_dir: '/Users/someone/work/proj',
          },
        })
      )
    );
  });

  it('warns that deleting the last memory takes the category with it', async () => {
    render(<MemorySection />);
    await screen.findByText('development');
    fireEvent.click(screen.getByRole('button', { name: /show the 1 memory in development/i }));

    fireEvent.click(
      (await screen.findAllByRole('button', { name: /delete this memory from development/i }))[0]
    );

    const dialog = await screen.findByRole('dialog');
    expect((dialog.textContent ?? '').toLowerCase()).toContain('last memory');
  });

  it('tells the user there is no project rather than showing an empty local store', async () => {
    mockInventory.mockResolvedValue(inventoryResponse({ local: null }));
    render(<MemorySection />);
    await screen.findByText('clinical');

    expect(screen.getByText(/no project/i)).toBeTruthy();
  });

  it('says plainly when nothing has been remembered globally', async () => {
    mockInventory.mockResolvedValue(
      inventoryResponse({
        global: {
          scope: 'global',
          path: '/Users/someone/.config/biorouter/memory',
          exists: false,
          categories: [],
        },
      })
    );
    render(<MemorySection />);

    expect(await screen.findByText(/nothing has been remembered/i)).toBeTruthy();
  });

  it('surfaces a refused delete instead of pretending it worked', async () => {
    mockDeleteCategory.mockRejectedValue(new Error('the category changed since it was listed'));
    render(<MemorySection />);
    await screen.findByText('clinical');

    fireEvent.click(screen.getByRole('button', { name: /delete the global category clinical/i }));
    await screen.findByRole('dialog');
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(mockToastError).toHaveBeenCalled());
    // The row is still there: a failed delete must not disappear from the list.
    expect(screen.getByText('clinical')).toBeTruthy();
  });

  it('a failed load says so, and does not claim there is no project', async () => {
    mockInventory.mockRejectedValue(new Error('the daemon is not answering'));
    render(<MemorySection />);

    expect(await screen.findByText(/the daemon is not answering/i)).toBeTruthy();
    // "No project open" is a real, different state. Reporting it when the
    // request simply failed tells the user their memories are not there,
    // which for a store they are being asked to consent to reads of is the
    // one wrong answer.
    expect(screen.queryByText(/no project/i)).toBeNull();
    expect(screen.queryByText(/nothing has been remembered/i)).toBeNull();
  });

  it('a long category name and a long memory body cannot break the layout', async () => {
    const long = 'a'.repeat(400);
    mockInventory.mockResolvedValue(
      inventoryResponse({
        global: {
          scope: 'global',
          path: '/Users/someone/.config/biorouter/memory',
          exists: true,
          categories: [
            {
              name: long,
              entries: [entry(0, 'x'.repeat(2000))],
              size_bytes: 2000,
              modified: 1_780_000_000,
            },
          ],
        },
      })
    );
    render(<MemorySection />);

    const name = await screen.findByText(long);
    // A flex/grid item defaults to min-width:auto, which is what lets long
    // unbroken text push a row wider than its container; min-w-0 is the fix and
    // break-words alone is not enough.
    expect(name.className).toContain('min-w-0');
    expect(name.className).toContain('[overflow-wrap:anywhere]');

    fireEvent.click(screen.getByRole('button', { name: `Show the 1 memory in ${long}` }));
    const body = await screen.findByText('x'.repeat(2000));
    expect(body.className).toContain('[overflow-wrap:anywhere]');
  });
});
