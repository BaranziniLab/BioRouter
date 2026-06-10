import React, { useState, useEffect } from 'react';
import { AlertTriangle, StopCircle, PauseCircle, RotateCcw, Zap, AlertCircle } from './icons/app-icons';
import { Button } from './ui/button';
import { InterruptionMatch } from '../utils/interruptionDetector';

interface InterruptionHandlerProps {
  match: InterruptionMatch | null;
  onConfirmInterruption: () => void;
  onCancelInterruption: () => void;
  onRedirect?: (newMessage: string) => void;
  className?: string;
}

export const InterruptionHandler: React.FC<InterruptionHandlerProps> = ({
  match,
  onConfirmInterruption,
  onCancelInterruption,
  onRedirect,
  className = '',
}) => {
  const [redirectMessage, setRedirectMessage] = useState('');
  const [showRedirectInput, setShowRedirectInput] = useState(false);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    if (match) {
      setIsVisible(true);
      if (match.keyword.action === 'redirect') {
        setShowRedirectInput(true);
      } else {
        setShowRedirectInput(false);
        setRedirectMessage('');
      }
    } else {
      setIsVisible(false);
    }
  }, [match]);

  if (!match) {
    return null;
  }

  const getIcon = () => {
    switch (match.keyword.action) {
      case 'stop':
        return <StopCircle className="w-6 h-6 text-text-danger" />;
      case 'pause':
        return <PauseCircle className="w-6 h-6 text-text-warning" />;
      case 'redirect':
        return <RotateCcw className="w-6 h-6 text-text-info" />;
      default:
        return <AlertTriangle className="w-6 h-6 text-text-warning" />;
    }
  };

  const getActionColor = () => {
    switch (match.keyword.action) {
      case 'stop':
        return {
          bg: 'bg-background-danger/10',
          border: 'border-border-danger/40',
          text: 'text-text-danger',
          accent: 'text-text-danger',
        };
      case 'pause':
        return {
          bg: 'bg-background-warning/10',
          border: 'border-border-warning/40',
          text: 'text-text-warning',
          accent: 'text-text-warning',
        };
      case 'redirect':
        return {
          bg: 'bg-background-info/10',
          border: 'border-border-info/40',
          text: 'text-text-info',
          accent: 'text-text-info',
        };
      default:
        return {
          bg: 'bg-background-warning/10',
          border: 'border-border-warning/40',
          text: 'text-text-warning',
          accent: 'text-text-warning',
        };
    }
  };

  const colors = getActionColor();

  const handleConfirm = () => {
    if (showRedirectInput && onRedirect && redirectMessage.trim()) {
      onRedirect(redirectMessage.trim());
    } else {
      onConfirmInterruption();
    }
  };

  const getActionTitle = () => {
    switch (match.keyword.action) {
      case 'stop':
        return 'Stop Processing';
      case 'pause':
        return 'Pause Processing';
      case 'redirect':
        return 'Redirect Processing';
      default:
        return 'Interrupt Processing';
    }
  };

  const getActionDescription = () => {
    switch (match.keyword.action) {
      case 'stop':
        return 'This will immediately stop the current processing and clear any queued messages.';
      case 'pause':
        return 'This will pause the current processing. Queued messages will be preserved.';
      case 'redirect':
        return 'This will stop current processing and redirect to a new task.';
      default:
        return 'This will interrupt the current processing.';
    }
  };

  return (
    <div
      className={`fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4 ${className}`}
    >
      <div
        className={`w-full max-w-md mx-auto transition-all duration-300 ease-out ${
          isVisible ? 'scale-100 opacity-100' : 'scale-95 opacity-0'
        }`}
      >
        {/* Main card */}
        <div
          className={`rounded-xl border shadow-2xl backdrop-blur-xl ${colors.bg} ${colors.border}`}
        >
          {/* Header */}
          <div className="p-6 border-b border-current/10">
            <div className="flex items-start gap-4">
              <div className="flex-shrink-0 p-2 rounded-lg bg-white/50 dark:bg-black/20">
                {getIcon()}
              </div>
              <div className="flex-1">
                <h3 className={`text-lg font-semibold ${colors.text}`}>{getActionTitle()}</h3>
                <p className={`text-sm mt-1 ${colors.accent}`}>Detected: "{match.matchedText}"</p>
              </div>
              <div
                className={`text-xs px-2 py-1 rounded-md bg-white/30 dark:bg-black/20 ${colors.text}`}
              >
                {Math.round(match.confidence * 100)}% confident
              </div>
            </div>
          </div>

          {/* Content */}
          <div className="p-6">
            <div className="flex items-start gap-3 mb-4">
              <AlertCircle className={`w-4 h-4 mt-0.5 flex-shrink-0 ${colors.accent}`} />
              <p className={`text-sm leading-relaxed ${colors.text}`}>{getActionDescription()}</p>
            </div>

            {/* Redirect input */}
            {showRedirectInput && (
              <div className="mb-4 space-y-2">
                <label className={`text-sm font-medium ${colors.text}`}>
                  New task or instruction:
                </label>
                <textarea
                  value={redirectMessage}
                  onChange={(e) => setRedirectMessage(e.target.value)}
                  placeholder="Enter your new instruction..."
                  className={`w-full px-3 py-2 border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-current/20 bg-white/50 dark:bg-black/20 ${colors.border} ${colors.text}`}
                  rows={3}
                  autoFocus
                />
              </div>
            )}

            {/* Confidence indicator */}
            <div className="mb-4">
              <div className="flex items-center justify-between text-xs mb-1">
                <span className={colors.accent}>Detection Confidence</span>
                <span className={colors.text}>{Math.round(match.confidence * 100)}%</span>
              </div>
              <div className="w-full bg-white/30 dark:bg-black/20 rounded-full h-2">
                <div
                  className={`h-2 rounded-full transition-all duration-500 ${
                    match.confidence > 0.8
                      ? 'bg-background-success'
                      : match.confidence > 0.6
                        ? 'bg-background-warning'
                        : 'bg-background-danger'
                  }`}
                  style={{ width: `${match.confidence * 100}%` }}
                />
              </div>
            </div>
          </div>

          {/* Actions */}
          <div className="p-6 pt-0 flex gap-3">
            <Button
              variant="ghost"
              onClick={onCancelInterruption}
              className={`flex-1 hover:bg-white/20 dark:hover:bg-black/20 ${colors.text}`}
            >
              Cancel
            </Button>
            <Button
              onClick={handleConfirm}
              disabled={showRedirectInput && !redirectMessage.trim()}
              className={`flex-1 bg-white/80 hover:bg-white dark:bg-white/10 dark:hover:bg-white/20 ${colors.text} font-medium transition-all duration-200`}
            >
              <Zap className="w-4 h-4 mr-2" />
              {showRedirectInput
                ? 'Redirect'
                : match.keyword.action === 'stop'
                  ? 'Stop'
                  : 'Confirm'}
            </Button>
          </div>
        </div>

        {/* Backdrop hint */}
        <div className="text-center mt-4">
          <p className="text-xs text-white/60">
            Click outside or press Cancel to continue current processing
          </p>
        </div>
      </div>
    </div>
  );
};

export default InterruptionHandler;
