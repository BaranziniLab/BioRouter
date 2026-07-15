import type { UpdaterState } from './updaterState';

export const OPEN_UPDATE_MODAL_EVENT = 'biorouter:open-update-modal';

export function requestUpdateModal(state: UpdaterState) {
  window.dispatchEvent(
    new CustomEvent<UpdaterState>(OPEN_UPDATE_MODAL_EVENT, {
      detail: state,
    })
  );
}
