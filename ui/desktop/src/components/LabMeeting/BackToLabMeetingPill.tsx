import React from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { useOptionalLabMeeting } from '../../contexts/LabMeetingContext';
import { Users } from '../icons/app-icons';

export const BackToLabMeetingPill: React.FC = () => {
  const lab = useOptionalLabMeeting();
  const navigate = useNavigate();
  const loc = useLocation();
  if (!lab) return null;
  if (loc.pathname === '/lab-meeting') return null;
  if (lab.state.windows.length === 0) return null;
  const onBoard = lab.state.windows.filter((w) => !w.isTucked).length;
  return (
    <button
      type="button"
      onClick={() => navigate('/lab-meeting')}
      className="fixed bottom-4 right-4 z-50 inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-background-default border border-border-subtle shadow-lg hover:bg-background-medium transition-colors text-xs"
      title="Back to Lab Meeting"
    >
      <Users className="w-3.5 h-3.5" />
      Back to Lab Meeting · {onBoard}
    </button>
  );
};
