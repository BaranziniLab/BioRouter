/**
 * App icon library — Lucide React (ISC, lucide.dev)
 * All icons render at strokeWidth=1.5 for a consistent light/outline appearance.
 */

import React from 'react';
import {
  Activity as _Activity,
  AlertCircle as _AlertCircle,
  AlertTriangle as _AlertTriangle,
  AppWindow as _AppWindow,
  AppWindowMac as _AppWindowMac,
  Archive as _Archive,
  ArrowLeft as _ArrowLeft,
  BookOpen as _BookOpen,
  Bot as _Bot,
  Brain as _Brain,
  Calendar as _Calendar,
  Check as _Check,
  CheckCircle as _CheckCircle,
  ChevronDown as _ChevronDown,
  ChevronLeft as _ChevronLeft,
  ChevronRight as _ChevronRight,
  ChevronUp as _ChevronUp,
  Circle as _Circle,
  CircleDotDashed as _CircleDotDashed,
  Clock as _Clock,
  Code as _Code,
  Copy as _Copy,
  Database as _Database,
  Download as _Download,
  Edit as _Edit,
  Edit2 as _Edit2,
  ExternalLink as _ExternalLink,
  Eye as _Eye,
  File as _File,
  FileText as _FileText,
  FlaskConical as _FlaskConical,
  Folder as _Folder,
  FolderDot as _FolderDot,
  FolderKey as _FolderKey,
  Github as _Github,
  Globe as _Globe,
  GripVertical as _GripVertical,
  HeartPulse as _HeartPulse,
  History as _History,
  Home as _Home,
  Image as _Image,
  Info as _Info,
  Layers as _Layers,
  LayoutDashboard as _LayoutDashboard,
  Link as _Link,
  Loader2 as _Loader2,
  LoaderCircle as _LoaderCircle,
  Lock as _Lock,
  Maximize2 as _Maximize2,
  Minimize2 as _Minimize2,
  MessageSquare as _MessageSquare,
  MessageSquareText as _MessageSquareText,
  Minus as _Minus,
  Monitor as _Monitor,
  Moon as _Moon,
  Music as _Music,
  Package as _Package,
  Palette as _Palette,
  PanelLeftIcon as _PanelLeftIcon,
  Pause as _Pause,
  PauseCircle as _PauseCircle,
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
// Light wrapper — sets strokeWidth=1.5 as the default; caller can override.
// ---------------------------------------------------------------------------
const light =
  (Icon: LucideIcon): React.FC<LucideProps> =>
  ({ strokeWidth = 1.5, ...props }) =>
    <Icon strokeWidth={strokeWidth} {...props} />;

// ---------------------------------------------------------------------------
// Named exports — same names as the original file so no consumer changes.
// ---------------------------------------------------------------------------

export const Activity = light(_Activity);
export const AlertCircle = light(_AlertCircle);
export const Info = light(_Info);
export const AlertTriangle = light(_AlertTriangle);
export const AppWindow = light(_AppWindow);
export const AppWindowMac = light(_AppWindowMac);
export const Archive = light(_Archive);
export const ArrowLeft = light(_ArrowLeft);
export const BookOpen = light(_BookOpen);
export const Bot = light(_Bot);
export const Brain = light(_Brain);
export const Calendar = light(_Calendar);
export const Check = light(_Check);
export const CheckIcon = Check;
export const CheckCircle = light(_CheckCircle);
export const ChevronUp = light(_ChevronUp);
export const ChevronDown = light(_ChevronDown);
export const ChevronDownIcon = ChevronDown;
export const ChevronRight = light(_ChevronRight);
export const ChevronRightIcon = ChevronRight;
export const ChevronLeft = light(_ChevronLeft);
export const CircleIcon = light(_Circle);
export const CircleDotDashed = light(_CircleDotDashed);
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
export const File = light(_File);
export const FileText = light(_FileText);
export const FlaskConical = light(_FlaskConical);
export const Folder = light(_Folder);
export const FolderDot = light(_FolderDot);
export const FolderKey = light(_FolderKey);
export const Github = light(_Github);
export const Globe = light(_Globe);
export const GripVertical = light(_GripVertical);
export const HeartPulse = light(_HeartPulse);
export const History = light(_History);
export const Home = light(_Home);
export const Image = light(_Image);
export const Layers = light(_Layers);
export const LayoutDashboard = light(_LayoutDashboard);
export const Link = light(_Link);
export const Loader2 = light(_Loader2);
export const LoaderCircle = light(_LoaderCircle);
export const Lock = light(_Lock);
export const Maximize2 = light(_Maximize2);
export const Minimize2 = light(_Minimize2);
export const MessageSquare = light(_MessageSquare);
export const MessageSquareText = light(_MessageSquareText);
export const Minus = light(_Minus);
export const Monitor = light(_Monitor);
export const Moon = light(_Moon);
export const Music = light(_Music);
export const Package = light(_Package);
export const Palette = light(_Palette);
export const PanelLeftIcon = light(_PanelLeftIcon);
export const Pause = light(_Pause);
export const PauseCircle = light(_PauseCircle);
export const Pill = light(_Pill);
export const Play = light(_Play);
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
// The original 'Square' icon was visually a circle-with-square (stop button),
// which corresponds to lucide's StopCircle. Keep both names pointing to it.
export const Square = light(_StopCircle);
export const StopCircle = Square;
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
// LucideIcon type re-export — kept for consumers that import the type.
// ---------------------------------------------------------------------------
export type { LucideIcon };
