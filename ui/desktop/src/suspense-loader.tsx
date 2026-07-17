import { BioRouterMark } from './components/icons/BioRouterMark';

export default function SuspenseLoader() {
  return (
    <div className="flex flex-col items-start justify-end w-screen h-screen overflow-hidden p-6 page-transition">
      <div className="flex gap-2 items-center justify-end">
        <BioRouterMark className="h-8 w-8" />
        <span className="text-text-muted">Loading...</span>
      </div>
    </div>
  );
}
