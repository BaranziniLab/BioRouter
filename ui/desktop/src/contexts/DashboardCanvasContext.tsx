import { createContext, useContext } from 'react';

/**
 * True only when a component is rendered INSIDE a Dashboard canvas chat window
 * (`ChatWindow`). This is intentionally distinct from `useOptionalDashboard()`:
 * the `DashboardProvider` wraps the entire app (so the dashboard API is
 * reachable from every route), but a component is only *on the canvas* when a
 * `ChatWindow` provides this context.
 *
 * Used by `useDiverge` so that:
 *  - Diverging from the single-chat (Hub/Pair) view opens a new Electron window.
 *  - Diverging from a Dashboard canvas window spawns a new on-canvas chat box.
 *
 * Without this, chat-side diverges would leak into the dashboard's persisted
 * window list because the dashboard context is always present.
 */
export const DashboardCanvasContext = createContext<boolean>(false);

export const useInDashboardCanvas = (): boolean => useContext(DashboardCanvasContext);
