interface WorkflowHeaderProps {
  title: string;
}

export function WorkflowHeader({ title }: WorkflowHeaderProps) {
  return (
    <div className="flex items-center px-4 py-1 border-b border-border-subtle/50">
      <div className="flex items-center ml-6">
        <span className="text-sm">
          <span className="text-text-muted">Workflow</span>{' '}
          <span className="text-text-default">{title}</span>
        </span>
      </div>
    </div>
  );
}
