interface OnboardingSectionLabelProps {
  category: 'institutional' | 'local' | 'commercial';
  label: string;
}

const DOT_CLASSES: Record<OnboardingSectionLabelProps['category'], string> = {
  institutional: 'bg-indigo-500',
  local: 'bg-emerald-500',
  commercial: 'bg-amber-500',
};

export default function OnboardingSectionLabel({ category, label }: OnboardingSectionLabelProps) {
  return (
    <div className="flex items-center gap-2 mb-1">
      <span className={`w-1.5 h-1.5 rounded-full ${DOT_CLASSES[category]}`} aria-hidden />
      <p className="text-[11px] font-medium uppercase tracking-wider text-text-muted">{label}</p>
    </div>
  );
}
