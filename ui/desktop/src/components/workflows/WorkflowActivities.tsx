import { Card } from '../ui/card';
import MarkdownContent from '../MarkdownContent';
import { substituteParameters } from '../../utils/providerUtils';

interface WorkflowActivitiesProps {
  append: (text: string) => void;
  activities: string[] | null;
  title?: string;
  parameterValues?: Record<string, string>;
}

export default function WorkflowActivities({
  append,
  activities,
  parameterValues = {},
}: WorkflowActivitiesProps) {
  const pills = activities || [];

  // Find any pill that starts with "message:"
  const messagePillIndex = pills.findIndex((pill) => pill.toLowerCase().startsWith('message:'));

  // Extract the message pill and the remaining pills
  const messagePill = messagePillIndex >= 0 ? pills[messagePillIndex] : null;
  const remainingPills =
    messagePillIndex >= 0
      ? [...pills.slice(0, messagePillIndex), ...pills.slice(messagePillIndex + 1)]
      : pills;

  // If we have activities or instructions (workflow mode), show a simplified version without greeting
  if (activities && activities.length > 0) {
    return (
      <div className="flex flex-col px-6">
        {messagePill && (
          <div className="mb-4 p-3 rounded-lg border border-border-subtle animate-[appear_180ms_var(--ease-out)_forwards]">
            <MarkdownContent
              content={substituteParameters(
                messagePill.replace(/^message:/i, '').trim(),
                parameterValues
              )}
              className="text-sm"
            />
          </div>
        )}

        <div className="flex flex-wrap gap-2 animate-[appear_180ms_var(--ease-out)_forwards]">
          {remainingPills.map((content, index) => {
            const substitutedContent = substituteParameters(content, parameterValues);
            return (
              <Card
                key={index}
                onClick={() => append(substitutedContent)}
                title={substitutedContent.length > 60 ? substitutedContent : undefined}
                className="cursor-pointer px-3 py-1.5 text-sm hover:bg-background-medium transition-colors"
              >
                {substitutedContent.length > 60
                  ? substitutedContent.slice(0, 60) + '...'
                  : substitutedContent}
              </Card>
            );
          })}
        </div>
      </div>
    );
  }

  return null;
}
