import React from 'react';

export function BaseModal({
  isOpen,
  title,
  children,
  actions,
}: {
  isOpen: boolean;
  title?: string;
  children: React.ReactNode;
  actions: React.ReactNode; // Buttons for actions
}) {
  if (!isOpen) return null;

  return (
    <div className="biorouter-modal-overlay fixed inset-0 z-[9999]">
      <div className="biorouter-modal-surface fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[440px] bg-background-default overflow-hidden p-[16px] pt-[24px] pb-0">
        <div className="px-8 pb-0 space-y-8">
          {/* Header */}
          {title && (
            <div className="flex">
              <h2 className="text-base font-semibold text-text-default">{title}</h2>
            </div>
          )}

          {/* Content */}
          {children && <div className="px-8">{children}</div>}

          {/* Actions */}
          <div className="mt-[8px] ml-[-24px] mr-[-24px] pt-[16px]">{actions}</div>
        </div>
      </div>
    </div>
  );
}
