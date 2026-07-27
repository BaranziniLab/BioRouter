/**
 * App icon library — Lucide React (ISC, lucide.dev)
 * All icons render at strokeWidth=1.5 for a consistent light/outline appearance.
 */

import React from 'react';
import {
  Activity as _Activity,
  AlertCircle as _AlertCircle,
  AlertTriangle as _AlertTriangle,
  AlignLeft as _AlignLeft,
  AppWindow as _AppWindow,
  AppWindowMac as _AppWindowMac,
  Archive as _Archive,
  ArrowDown as _ArrowDown,
  ArrowLeft as _ArrowLeft,
  ArrowUp as _ArrowUp,
  BookMarked as _BookMarked,
  BookOpen as _BookOpen,
  Bookmark as _Bookmark,
  BookmarkPlus as _BookmarkPlus,
  Bot as _Bot,
  Brain as _Brain,
  Calendar as _Calendar,
  Camera as _Camera,
  Check as _Check,
  CheckCircle as _CheckCircle,
  CheckCircle2 as _CheckCircle2,
  ChevronDown as _ChevronDown,
  ChevronLeft as _ChevronLeft,
  ChevronRight as _ChevronRight,
  ChevronUp as _ChevronUp,
  ChevronsDownUp as _ChevronsDownUp,
  Circle as _Circle,
  CircleDotDashed as _CircleDotDashed,
  Clipboard as _Clipboard,
  ClipboardList as _ClipboardList,
  Clock as _Clock,
  Code as _Code,
  Copy as _Copy,
  Database as _Database,
  Download as _Download,
  Edit as _Edit,
  Edit2 as _Edit2,
  ExternalLink as _ExternalLink,
  Eye as _Eye,
  EyeOff as _EyeOff,
  File as _File,
  FileCode2 as _FileCode2,
  FilePlus as _FilePlus,
  FileSpreadsheet as _FileSpreadsheet,
  FileStack as _FileStack,
  FileText as _FileText,
  FileX as _FileX,
  FlaskConical as _FlaskConical,
  Folder as _Folder,
  FolderDot as _FolderDot,
  FolderInput as _FolderInput,
  FolderKey as _FolderKey,
  FolderOpen as _FolderOpen,
  FolderPlus as _FolderPlus,
  FolderTree as _FolderTree,
  Gauge as _Gauge,
  GitBranch as _GitBranch,
  Github as _Github,
  Globe as _Globe,
  GripVertical as _GripVertical,
  HeartPulse as _HeartPulse,
  History as _History,
  Home as _Home,
  Image as _Image,
  Info as _Info,
  Layers as _Layers,
  Link as _Link,
  Loader2 as _Loader2,
  LoaderCircle as _LoaderCircle,
  Lock as _Lock,
  Maximize2 as _Maximize2,
  MessageSquare as _MessageSquare,
  MessageSquarePlus as _MessageSquarePlus,
  MessageSquareText as _MessageSquareText,
  Monitor as _Monitor,
  Moon as _Moon,
  MoreHorizontal as _MoreHorizontal,
  Music as _Music,
  Package as _Package,
  Palette as _Palette,
  PanelLeftIcon as _PanelLeftIcon,
  Pause as _Pause,
  PauseCircle as _PauseCircle,
  Pencil as _Pencil,
  Pill as _Pill,
  Play as _Play,
  Plus as _Plus,
  Puzzle as _Puzzle,
  QrCode as _QrCode,
  RefreshCw as _RefreshCw,
  Rocket as _Rocket,
  RotateCcw as _RotateCcw,
  Save as _Save,
  ScrollText as _ScrollText,
  Search as _Search,
  SearchCode as _SearchCode,
  Send as _Send,
  Settings as _Settings,
  Share2 as _Share2,
  Sliders as _Sliders,
  SlidersHorizontal as _SlidersHorizontal,
  Sparkles as _Sparkles,
  Square as _Square,
  SquareStack as _SquareStack,
  StopCircle as _StopCircle,
  Sun as _Sun,
  Target as _Target,
  Terminal as _Terminal,
  Tornado as _Tornado,
  Trash2 as _Trash2,
  Upload as _Upload,
  Users as _Users,
  Video as _Video,
  Workflow as _Workflow,
  Wrench as _Wrench,
  X as _X,
  Zap as _Zap,
  type LucideIcon,
  type LucideProps,
} from 'lucide-react';

// ---------------------------------------------------------------------------
// Light wrapper — enforces the canonical icon contract (design.md §3.9):
// every icon renders at strokeWidth=1.5 and is monochrome via `currentColor`.
// Both are pinned *after* the prop spread so a caller can never reintroduce a
// second stroke weight (DR-53) or a hardcoded fill. `currentColor` still lets
// callers tint an icon through CSS `color` (className/style) — it only blocks a
// hex being injected via the `color` prop.
// ---------------------------------------------------------------------------
const light = (Icon: LucideIcon): React.FC<LucideProps> => {
  const Wrapped: React.FC<LucideProps> = (props) => (
    <Icon {...props} strokeWidth={1.5} color="currentColor" />
  );
  // Name the wrapper so React DevTools (and react/display-name) can identify it.
  Wrapped.displayName = `light(${Icon.displayName ?? Icon.name ?? 'Icon'})`;
  return Wrapped;
};

// ---------------------------------------------------------------------------
// Named exports — same names as the original file so no consumer changes.
// ---------------------------------------------------------------------------

export const Activity = light(_Activity);
export const AlertCircle = light(_AlertCircle);
export const Info = light(_Info);
export const AlertTriangle = light(_AlertTriangle);
// The session-review mark the cohesion spec draws (three descending rules).
export const AlignLeft = light(_AlignLeft);
export const AppWindow = light(_AppWindow);
export const AppWindowMac = light(_AppWindowMac);
export const Archive = light(_Archive);
export const ArrowDown = light(_ArrowDown);
export const ArrowLeft = light(_ArrowLeft);
export const ArrowUp = light(_ArrowUp);
export const BookMarked = light(_BookMarked);
export const BookOpen = light(_BookOpen);
export const Bookmark = light(_Bookmark);
export const BookmarkPlus = light(_BookmarkPlus);
export const Bot = light(_Bot);
export const Brain = light(_Brain);
export const Calendar = light(_Calendar);
export const Camera = light(_Camera);
export const Check = light(_Check);
export const CheckIcon = Check;
export const CheckCircle = light(_CheckCircle);
export const CheckCircle2 = light(_CheckCircle2);
export const ChevronUp = light(_ChevronUp);
export const ChevronDown = light(_ChevronDown);
export const ChevronDownIcon = ChevronDown;
export const ChevronRight = light(_ChevronRight);
export const ChevronRightIcon = ChevronRight;
export const ChevronLeft = light(_ChevronLeft);
export const ChevronsDownUp = light(_ChevronsDownUp);
export const CircleIcon = light(_Circle);
export const CircleDotDashed = light(_CircleDotDashed);
export const Clipboard = light(_Clipboard);
export const ClipboardList = light(_ClipboardList);
export const Clock = light(_Clock);
export const Code = light(_Code);
export const CodeAnalysis = light(_SearchCode);
export const Copy = light(_Copy);
export const Database = light(_Database);
export const Download = light(_Download);
export const Edit = light(_Edit);
export const Edit2 = light(_Edit2);
export const ExternalLink = light(_ExternalLink);
export const Eye = light(_Eye);
export const EyeOff = light(_EyeOff);
export const File = light(_File);
export const FileCode2 = light(_FileCode2);
export const FilePlus = light(_FilePlus);
export const FileSpreadsheet = light(_FileSpreadsheet);
export const FileStack = light(_FileStack);
export const FileText = light(_FileText);
export const FileX = light(_FileX);
export const FlaskConical = light(_FlaskConical);
export const Folder = light(_Folder);
export const FolderDot = light(_FolderDot);
export const FolderInput = light(_FolderInput);
export const FolderKey = light(_FolderKey);
export const FolderOpen = light(_FolderOpen);
export const FolderPlus = light(_FolderPlus);
export const FolderTree = light(_FolderTree);
export const Gauge = light(_Gauge);
export const GitBranch = light(_GitBranch);
export const Github = light(_Github);
export const Globe = light(_Globe);
export const GripVertical = light(_GripVertical);
export const HeartPulse = light(_HeartPulse);
export const History = light(_History);
export const Home = light(_Home);
export const Image = light(_Image);
export const Layers = light(_Layers);
export const Link = light(_Link);
export const Loader2 = light(_Loader2);
export const LoaderCircle = light(_LoaderCircle);
export const Lock = light(_Lock);
export const Maximize2 = light(_Maximize2);
export const MessageSquare = light(_MessageSquare);
export const MessageSquarePlus = light(_MessageSquarePlus);
export const MessageSquareText = light(_MessageSquareText);
export const Monitor = light(_Monitor);
export const Moon = light(_Moon);
export const MoreHorizontal = light(_MoreHorizontal);
export const Music = light(_Music);
export const Package = light(_Package);
export const Palette = light(_Palette);
export const PanelLeftIcon = light(_PanelLeftIcon);
export const Pause = light(_Pause);
export const PauseCircle = light(_PauseCircle);
export const Pencil = light(_Pencil);
export const Pill = light(_Pill);
export const Play = light(_Play);
// `Plus` means exactly one thing: *new session*. Anything else that "adds" or
// "opens" needs its own mark — see `NewWindow` (DR-53: one glyph, one meaning).
export const Plus = light(_Plus);
export const PlusIcon = Plus;
export const Puzzle = light(_Puzzle);
export const QrCode = light(_QrCode);
export const RefreshCw = light(_RefreshCw);
export const Rocket = light(_Rocket);
export const RotateCcw = light(_RotateCcw);
export const Save = light(_Save);
export const ScrollText = light(_ScrollText);
export const Search = light(_Search);
export const SearchIcon = Search;
export const Send = light(_Send);
export const Settings = light(_Settings);
export const Share2 = light(_Share2);
export const Sliders = light(_Sliders);
export const SlidersHorizontal = light(_SlidersHorizontal);
export const Sparkles = light(_Sparkles);
// Opening a *new window* — the ⧉ mark the cohesion spec draws for the titlebar
// control (two offset windows). Deliberately not `Plus` (that is New Session),
// not `AppWindow` (that is the Applications route), and not `Copy`, whose
// geometry the spec's drawing actually matches but which already means copy.
export const NewWindow = light(_SquareStack);
// The original 'Square' icon was visually a circle-with-square (stop button),
// which corresponds to lucide's StopCircle. Keep both names pointing to it.
export const Square = light(_StopCircle);
export const StopCircle = Square;
export const StopSquare = light(_Square);
export const Sun = light(_Sun);
export const Target = light(_Target);
export const Terminal = light(_Terminal);
export const Tornado = light(_Tornado);
export const Trash2 = light(_Trash2);
export const Upload = light(_Upload);
export const Users = light(_Users);
export const Video = light(_Video);
// No direct 'Pipeline' in lucide; Workflow is the closest visual match.
export const Pipeline = light(_Workflow);
export const Wrench = light(_Wrench);
export const X = light(_X);
export const XIcon = X;
export const Zap = light(_Zap);

// ---------------------------------------------------------------------------
// Custom icons — hand-crafted SVGs not available in lucide-react.
// ---------------------------------------------------------------------------

/** Central node with rays — represents a knowledge graph / KB. */
export const KnowledgeIcon: React.FC<LucideProps> = (props) => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    strokeLinecap="round"
    strokeLinejoin="round"
    {...props}
    stroke="currentColor"
    strokeWidth={1.5}
  >
    <circle cx="12" cy="12" r="3" />
    <circle cx="5" cy="6" r="1.6" />
    <circle cx="19" cy="6" r="1.6" />
    <circle cx="6" cy="18" r="1.6" />
    <circle cx="18" cy="18" r="1.6" />
    <path d="M10 10.5L6 7M14 10.5l4-3.5M10 14l-4 3M14 14l4 3" />
  </svg>
);

// ---------------------------------------------------------------------------
// LucideIcon type re-export — kept for consumers that import the type.
// ---------------------------------------------------------------------------
export type { LucideIcon, LucideProps };
