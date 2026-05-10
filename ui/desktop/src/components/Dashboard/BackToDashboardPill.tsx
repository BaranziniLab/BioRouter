import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useOptionalDashboard } from '../../contexts/DashboardContext';
import { LayoutDashboard } from '../icons/app-icons';

export const BackToDashboardPill: React.FC = () => {
  const lab = useOptionalDashboard();
  const navigate = useNavigate();
  const loc = useLocation();
  if (!lab) return null;
  if (loc.pathname === '/dashboard') return null;
  if (lab.state.windows.length === 0) return null;
  const onBoard = lab.state.windows.filter((w) => !w.isTucked).length;
  return (
    <button
      type="button"
      onClick={() => navigate('/dashboard')}
      className="fixed bottom-4 right-4 z-50 inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-background-default border border-border-subtle shadow-lg hover:bg-background-medium transition-colors text-xs"
      title="Back to Dashboard"
    >
      <LayoutDashboard className="w-3.5 h-3.5" />
      Back to Dashboard · {onBoard}
    </button>
  );
};
