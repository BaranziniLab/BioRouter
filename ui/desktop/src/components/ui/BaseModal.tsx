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
      <div className="biorouter-modal-surface fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[440px] bg-background-default overflow-hidden p-6">
        <div className="space-y-8">
          {/* Header */}
          {title && (
            <div className="flex">
              <h2 className="text-base font-semibold text-text-default">{title}</h2>
            </div>
          )}

          {/* Content */}
          {children && <div>{children}</div>}

          {/* Actions */}
          <div className="-mx-6 pt-4">{actions}</div>
        </div>
      </div>
    </div>
  );
}
