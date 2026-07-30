import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import ImagePreview from './ImagePreview';
import { InlineImage } from './InlineImage';
import { extractImagePaths, removeImagePathsFromText } from '../utils/imageUtils';
import { getTextContent } from '../types/message';
import { Message } from '../api';
import MessageCopyLink from './MessageCopyLink';
import { formatMessageTimestamp } from '../utils/timeUtils';
import { Edit } from './icons/app-icons';
import { Button } from './ui/button';
import { ResourceRefChip, ResourceRefText } from './ResourceRefChip';
import { joinComposerText, removeComposerRefAt, splitComposerText } from '../utils/composerRefs';

interface UserMessageProps {
  message: Message;
  onMessageUpdate?: (messageId: string, newContent: string, editType?: 'diverge' | 'edit') => void;
}

export default function UserMessage({ message, onMessageUpdate }: UserMessageProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState('');
  const [hasBeenEdited, setHasBeenEdited] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Extract text content from the message
  const textContent = getTextContent(message);

  // Extract image paths from the message
  const imagePaths = extractImagePaths(textContent);

  // Extract structured ImageContent blocks (new sessions only; pre-v-next sessions have no image blocks)
  const imageContentBlocks = useMemo(
    () => message.content.filter((c) => c.type === 'image'),
    [message.content]
  );

  // Remove image paths and injected context blocks from display text
  const displayText = useMemo(
    () =>
      removeImagePathsFromText(textContent, imagePaths)
        .replace(/<info-msg>[\s\S]*?<\/info-msg>(\s*\\?")?/gi, '')
        .trim(),
    [textContent, imagePaths]
  );

  // Issue #65 — the edit box is a second composer, so it follows the same rule:
  // prose in the textarea, references as chips. Without this, "Edit" would be
  // the one place the `<biorouter-ref …>` markup still leaks, and a user tidying
  // their sentence would delete half a tag and silently lose the reference.
  const editRefs = useMemo(() => splitComposerText(editContent).refs, [editContent]);
  const editBody = useMemo(() => splitComposerText(editContent).body, [editContent]);

  // Memoize the timestamp
  const timestamp = useMemo(() => formatMessageTimestamp(message.created), [message.created]);

  // Effect to handle message content changes and ensure persistence
  useEffect(() => {
    // If we're not editing, update the edit content to match the current message
    if (!isEditing) {
      setEditContent(displayText);
    }
  }, [message.content, displayText, message.id, isEditing]);

  // Initialize edit mode with current message content
  const initializeEditMode = useCallback(() => {
    setEditContent(displayText);
    setError(null);
    window.electron.logInfo(`Entering edit mode with content: ${displayText}`);
  }, [displayText]);

  // Handle edit button click
  const handleEditClick = useCallback(() => {
    const newEditingState = !isEditing;
    setIsEditing(newEditingState);

    // Initialize edit content when entering edit mode
    if (newEditingState) {
      initializeEditMode();
      window.electron.logInfo(`Edit interface shown for message: ${message.id}`);

      // Focus the textarea after a brief delay to ensure it's rendered
      setTimeout(() => {
        if (textareaRef.current) {
          textareaRef.current.focus();
          textareaRef.current.setSelectionRange(
            textareaRef.current.value.length,
            textareaRef.current.value.length
          );
        }
      }, 50);
    }

    window.electron.logInfo(`Edit state toggled: ${newEditingState} for message: ${message.id}`);
  }, [isEditing, initializeEditMode, message.id]);

  // Handle content changes in edit mode
  const handleContentChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      // The textarea holds the prose; the references ride along untouched.
      const newContent = joinComposerText(e.target.value, editRefs);
      setEditContent(newContent);
      setError(null); // Clear any previous errors
      window.electron.logInfo(`Content changed: ${newContent}`);
    },
    [editRefs]
  );

  const handleRemoveEditReference = useCallback((index: number) => {
    setEditContent((current) => removeComposerRefAt(current, index));
  }, []);

  const handleSave = useCallback(
    (editType: 'diverge' | 'edit' = 'diverge') => {
      if (editContent.trim().length === 0) {
        setError('Message cannot be empty');
        return;
      }

      setIsEditing(false);

      if (editContent.trim() === displayText.trim()) {
        return;
      }

      if (onMessageUpdate && message.id) {
        onMessageUpdate(message.id, editContent, editType);
        setHasBeenEdited(true);
      }
    },
    [editContent, displayText, onMessageUpdate, message.id]
  );

  // Handle cancel action
  const handleCancel = useCallback(() => {
    window.electron.logInfo('Cancel clicked - reverting to original content');
    setIsEditing(false);
    setEditContent(displayText); // Reset to original content
    setError(null);
  }, [displayText]);

  // Handle keyboard events for accessibility
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      window.electron.logInfo(
        `Key pressed: ${e.key}, metaKey: ${e.metaKey}, ctrlKey: ${e.ctrlKey}`
      );

      if (e.key === 'Escape') {
        e.preventDefault();
        handleCancel();
      } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        window.electron.logInfo('Cmd+Enter detected, calling handleSave');
        handleSave();
      }
    },
    [handleCancel, handleSave]
  );

  // Auto-resize textarea based on content
  useEffect(() => {
    if (textareaRef.current && isEditing) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
    }
  }, [editContent, isEditing]);

  return (
    <div className="w-full mt-[16px] opacity-0 animate-[appear_150ms_var(--ease-out)_forwards]">
      <div className="flex flex-col group">
        {isEditing ? (
          // Truly wide, centered, in-place edit box replacing the bubble
          <div className="w-full max-w-4xl mx-auto bg-background-default text-text-default rounded-xl border border-border-subtle py-4 px-4 my-2 transition-all duration-200 ease-in-out">
            {editRefs.length > 0 && (
              <div
                data-testid="edit-reference-rail"
                className="mb-2 flex flex-wrap items-center gap-1.5"
              >
                {editRefs.map((ref, index) => (
                  <ResourceRefChip
                    key={`${ref.kind}:${ref.value}`}
                    refSpan={ref}
                    onRemove={() => handleRemoveEditReference(index)}
                  />
                ))}
              </div>
            )}
            <textarea
              ref={textareaRef}
              value={editBody}
              onChange={handleContentChange}
              onKeyDown={handleKeyDown}
              className="w-full resize-none bg-transparent text-text-default placeholder:text-text-muted border border-border-subtle rounded-lg focus:border-border-strong transition-all duration-200 text-base leading-relaxed"
              style={{
                minHeight: '120px',
                maxHeight: '300px',
                padding: '16px',
                fontFamily: 'inherit',
                lineHeight: '1.6',
                wordBreak: 'break-word',
                overflowWrap: 'break-word',
              }}
              placeholder="Edit your message..."
              aria-label="Edit message content"
              aria-describedby={error ? `error-${message.id}` : undefined}
            />
            {/* Error message */}
            {error && (
              <div
                id={`error-${message.id}`}
                className="text-text-danger text-xs mt-2 mb-2"
                role="alert"
                aria-live="polite"
              >
                {error}
              </div>
            )}
            <div className="flex justify-between items-center mt-4">
              <div className="text-xs text-text-muted">
                <span className="font-semibold">Edit in Place</span> updates this session •{' '}
                <span className="font-semibold">Diverge Session</span> creates a new session
              </div>
              <div className="flex gap-3">
                <Button onClick={handleCancel} variant="ghost" aria-label="Cancel editing">
                  Cancel
                </Button>
                <Button
                  onClick={() => handleSave('edit')}
                  variant="secondary"
                  aria-label="Edit message in place"
                  title="Update the message in this session"
                >
                  Edit in Place
                </Button>
                <Button
                  onClick={() => handleSave('diverge')}
                  aria-label="Diverge session with edited message"
                  title="Create a new session with the edited message"
                >
                  Diverge Session
                </Button>
              </div>
            </div>
          </div>
        ) : (
          // Normal message display
          <div className="message flex justify-end w-full">
            <div className="flex-col max-w-[85%] w-fit min-w-0">
              <div className="flex flex-col group min-w-0">
                {/* The user's turn is tinted only so the eye can find the boundary
                    when scrolling. It is NOT an accent surface — a solid coral block
                    shouts on a canvas whose whole thesis is calm (design.md §4.18). */}
                <div className="flex min-w-0 rounded-xl border border-border-subtle bg-background-medium px-4 py-2.5">
                  {/* min-w-0 is required on this flex item: overflow-wrap:break-word
                      (break-words) prevents *visual* overflow but does NOT reduce the
                      element's intrinsic min-content width, so a long unbroken token
                      (e.g. a comma-separated number list with no spaces) keeps the
                      flex item at full-token width and bleeds past the bubble. min-w-0
                      lets the item shrink so the break can happen. */}
                  {/* A sent message keeps its `<biorouter-ref …>` tags — they
                      are what the agent read, what a reload replays and what an
                      edit re-sends — so the transcript draws them as chips
                      rather than letting the user watch their own message come
                      back as XML. Anything the parser refuses stays visible as
                      the text it is, which is honest: the backend ignored it
                      too. */}
                  <div
                    ref={contentRef}
                    className="min-w-0 text-sm text-text-default whitespace-pre-wrap break-words leading-relaxed"
                  >
                    <ResourceRefText text={displayText} />
                  </div>
                </div>

                {/* Render images if any */}
                {imagePaths.length > 0 && (
                  <div className="flex flex-wrap gap-2 mt-2">
                    {imagePaths.map((imagePath, index) => (
                      <ImagePreview key={index} src={imagePath} alt={`Pasted image ${index + 1}`} />
                    ))}
                  </div>
                )}

                {/* Render structured ImageContent blocks (new sessions) */}
                {imageContentBlocks.length > 0 && (
                  <div className="flex flex-wrap gap-2 mt-2">
                    {imageContentBlocks.map((block, idx) =>
                      block.type === 'image' ? (
                        <InlineImage
                          key={idx}
                          kind="data"
                          data={block.data}
                          mimeType={block.mimeType}
                        />
                      ) : null
                    )}
                  </div>
                )}

                <div className="relative h-[22px] flex justify-end text-right">
                  <div className="absolute w-40 font-sans right-0 text-sm text-text-muted pt-1 transition-[transform,opacity] duration-150 group-hover:-translate-y-4 group-hover:opacity-0">
                    {timestamp}
                  </div>
                  <div className="absolute right-0 pt-1 flex items-center gap-2">
                    <button
                      onClick={handleEditClick}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          handleEditClick();
                        }
                      }}
                      className="flex items-center gap-1 font-sans text-sm text-text-muted hover:cursor-pointer hover:text-text-default transition-[transform,opacity,color] duration-150 opacity-0 group-hover:opacity-100 -translate-y-4 group-hover:translate-y-0 rounded"
                      aria-label={`Edit message: ${displayText.substring(0, 50)}${displayText.length > 50 ? '...' : ''}`}
                      aria-expanded={isEditing}
                    >
                      <Edit className="h-3 w-3" />
                      <span>Edit</span>
                    </button>
                    <MessageCopyLink text={displayText} contentRef={contentRef} />
                  </div>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Edited indicator */}
        {hasBeenEdited && !isEditing && (
          <div className="font-sans text-sm text-text-muted mt-1 text-right transition-opacity duration-200">
            Edited
          </div>
        )}
      </div>
    </div>
  );
}
