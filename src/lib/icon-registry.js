/**
 * Shared icon registry - maps a stable semantic name to both a Lucide component
 * and a Hugeicons data object, so the <Icon> wrapper can render either family
 * based on the user's icon-set setting.
 *
 * Coverage is intentionally incremental: a name may have a Lucide entry only.
 * <Icon> falls back to Lucide whenever a Hugeicons mapping is missing, so the UI
 * never breaks and the Hugeicons look can be extended one name at a time.
 *
 * Add a new icon:
 *   1. import its Lucide component + Hugeicons data below,
 *   2. add a `name: { lucide, huge }` entry,
 *   3. render it anywhere with `<Icon name="…" class="size-4" />`.
 */

// ── Lucide components ────────────────────────────────────────────────────────
import X from '@lucide/svelte/icons/x'
import Check from '@lucide/svelte/icons/check'
import CheckCircle2 from '@lucide/svelte/icons/check-circle-2'
import ChevronDown from '@lucide/svelte/icons/chevron-down'
import ChevronUp from '@lucide/svelte/icons/chevron-up'
import ChevronRight from '@lucide/svelte/icons/chevron-right'
import ChevronLeft from '@lucide/svelte/icons/chevron-left'
import Plus from '@lucide/svelte/icons/plus'
import Minus from '@lucide/svelte/icons/minus'
import Search from '@lucide/svelte/icons/search'
import Trash2 from '@lucide/svelte/icons/trash-2'
import LogOut from '@lucide/svelte/icons/log-out'
import Table2 from '@lucide/svelte/icons/table-2'
import RefreshCw from '@lucide/svelte/icons/refresh-cw'
import Copy from '@lucide/svelte/icons/copy'
import CopyPlus from '@lucide/svelte/icons/copy-plus'
import Loader2 from '@lucide/svelte/icons/loader-2'
import KeyRound from '@lucide/svelte/icons/key-round'
import ExternalLink from '@lucide/svelte/icons/external-link'
import Database from '@lucide/svelte/icons/database'
import Code2 from '@lucide/svelte/icons/code-2'
import Terminal from '@lucide/svelte/icons/terminal'
import BarChart2 from '@lucide/svelte/icons/bar-chart-2'
import Sparkles from '@lucide/svelte/icons/sparkles'
import GitBranch from '@lucide/svelte/icons/git-branch'
import Download from '@lucide/svelte/icons/download'
import Play from '@lucide/svelte/icons/play'
import Eye from '@lucide/svelte/icons/eye'
import EyeOff from '@lucide/svelte/icons/eye-off'
import Bot from '@lucide/svelte/icons/bot'
import AlertTriangle from '@lucide/svelte/icons/alert-triangle'
import AlertCircle from '@lucide/svelte/icons/alert-circle'
import ShieldCheck from '@lucide/svelte/icons/shield-check'
import Globe from '@lucide/svelte/icons/globe'
import Pencil from '@lucide/svelte/icons/pencil'
import Braces from '@lucide/svelte/icons/braces'
import Lock from '@lucide/svelte/icons/lock'
import LayoutTemplate from '@lucide/svelte/icons/layout-template'
import History from '@lucide/svelte/icons/history'
import ArrowRight from '@lucide/svelte/icons/arrow-right'
import Clock from '@lucide/svelte/icons/clock'
import Zap from '@lucide/svelte/icons/zap'
import Settings from '@lucide/svelte/icons/settings'
import Maximize2 from '@lucide/svelte/icons/maximize-2'
import Filter from '@lucide/svelte/icons/filter'
import MoreHorizontal from '@lucide/svelte/icons/more-horizontal'
import Menu from '@lucide/svelte/icons/menu'
import FolderOpen from '@lucide/svelte/icons/folder-open'
import Folder from '@lucide/svelte/icons/folder'
import FolderPlus from '@lucide/svelte/icons/folder-plus'
import FileCode from '@lucide/svelte/icons/file-code'
import Archive from '@lucide/svelte/icons/archive'
import ArrowUpCircle from '@lucide/svelte/icons/arrow-up-circle'
import ChevronsDown from '@lucide/svelte/icons/chevrons-down'
import ChevronsLeft from '@lucide/svelte/icons/chevrons-left'
import ChevronsRight from '@lucide/svelte/icons/chevrons-right'
import ChevronsUp from '@lucide/svelte/icons/chevrons-up'
import Cloud from '@lucide/svelte/icons/cloud'
import Command from '@lucide/svelte/icons/command'
import HardDrive from '@lucide/svelte/icons/hard-drive'
import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard'
import LockOpen from '@lucide/svelte/icons/lock-open'
import Moon from '@lucide/svelte/icons/moon'
import Radio from '@lucide/svelte/icons/radio'
import Server from '@lucide/svelte/icons/server'
import Settings2 from '@lucide/svelte/icons/settings-2'
import Sun from '@lucide/svelte/icons/sun'
import Undo2 from '@lucide/svelte/icons/undo-2'
import Unplug from '@lucide/svelte/icons/unplug'
import Wifi from '@lucide/svelte/icons/wifi'
import WifiOff from '@lucide/svelte/icons/wifi-off'
import ArrowLeft from '@lucide/svelte/icons/arrow-left'
import FlaskConical from '@lucide/svelte/icons/flask-conical'
import Workflow from '@lucide/svelte/icons/workflow'
import Share2 from '@lucide/svelte/icons/share-2'
import GitCompare from '@lucide/svelte/icons/git-compare'
import Blocks from '@lucide/svelte/icons/blocks'
import Plug from '@lucide/svelte/icons/plug'
import NotebookPen from '@lucide/svelte/icons/notebook-pen'
import BarChart3 from '@lucide/svelte/icons/bar-chart-3'
import Info from '@lucide/svelte/icons/info'
import ArrowDown from '@lucide/svelte/icons/arrow-down'
import ArrowUp from '@lucide/svelte/icons/arrow-up'
import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down'
import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right'
import ArrowDownToLine from '@lucide/svelte/icons/arrow-down-to-line'
import ArrowLeftToLine from '@lucide/svelte/icons/arrow-left-to-line'
import ArrowRightToLine from '@lucide/svelte/icons/arrow-right-to-line'
import ReplaceLucide from '@lucide/svelte/icons/replace'
import Bookmark from '@lucide/svelte/icons/bookmark'
import Box from '@lucide/svelte/icons/box'
import Bug from '@lucide/svelte/icons/bug'
import CircleSlash from '@lucide/svelte/icons/circle-slash'
import ClipboardCopy from '@lucide/svelte/icons/clipboard-copy'
import Cog from '@lucide/svelte/icons/cog'
import Columns3 from '@lucide/svelte/icons/columns-3'
import Crosshair from '@lucide/svelte/icons/crosshair'
import Eraser from '@lucide/svelte/icons/eraser'
import FileDown from '@lucide/svelte/icons/file-down'
import FileUp from '@lucide/svelte/icons/file-up'
import FileText from '@lucide/svelte/icons/file-text'
import FunctionSquare from '@lucide/svelte/icons/function-square'
import Hash from '@lucide/svelte/icons/hash'
import Infinity from '@lucide/svelte/icons/infinity'
import Keyboard from '@lucide/svelte/icons/keyboard'
import Layers from '@lucide/svelte/icons/layers'
import LayoutList from '@lucide/svelte/icons/layout-list'
import PanelLeftClose from '@lucide/svelte/icons/panel-left-close'
import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open'
import Link2 from '@lucide/svelte/icons/link-2'
import ListFilter from '@lucide/svelte/icons/list-filter'
import Network from '@lucide/svelte/icons/network'
import Package from '@lucide/svelte/icons/package'
import Pin from '@lucide/svelte/icons/pin'
import PinOff from '@lucide/svelte/icons/pin-off'
import RotateCcw from '@lucide/svelte/icons/rotate-ccw'
import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal'
import Square from '@lucide/svelte/icons/square'
import SquareCheck from '@lucide/svelte/icons/square-check'
import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down'
import ChevronsDownUp from '@lucide/svelte/icons/chevrons-down-up'
import ArrowDown01 from '@lucide/svelte/icons/arrow-down-0-1'
import ArrowDownAZ from '@lucide/svelte/icons/arrow-down-a-z'
import ArrowUp01 from '@lucide/svelte/icons/arrow-up-0-1'
import ArrowUpAZ from '@lucide/svelte/icons/arrow-up-a-z'
import WrapText from '@lucide/svelte/icons/wrap-text'
import ListOrdered from '@lucide/svelte/icons/list-ordered'
import FileDiff from '@lucide/svelte/icons/file-diff'

// ── Hugeicons data ───────────────────────────────────────────────────────────
import {
  Cancel01Icon, Tick01Icon, CheckmarkCircle02Icon, ArrowDown01Icon, ArrowUp01Icon,
  ArrowRight01Icon, ArrowLeft01Icon, Add01Icon, MinusSignIcon, Search01Icon,
  Delete02Icon, Logout01Icon, Table01Icon, GridTableIcon, RefreshIcon, Copy01Icon, Loading03Icon, Key01Icon,
  LinkSquare01Icon, Database01Icon, SourceCodeIcon, TerminalIcon, ChartLineData02Icon,
  SparklesIcon, GitBranchIcon, Download04Icon, PlayIcon, EyeIcon, EyeOffIcon,
  AiBrain01Icon, Alert02Icon, AlertCircleIcon, Shield01Icon, PencilEdit01Icon,
  BracketsIcon, SquareLock01Icon, DashboardSquare01Icon, HistoryIcon, Clock01Icon,
  FlashIcon, Settings01Icon, Maximize01Icon, FilterIcon, MoreHorizontalIcon,
  Menu01Icon, FolderOpenIcon, Folder01Icon, FolderAddIcon, FileScriptIcon, Archive01Icon, CircleArrowUp01Icon, ArrowDownDoubleIcon,
  ArrowLeftDoubleIcon, ArrowRightDoubleIcon, ArrowUpDoubleIcon, CloudIcon, CommandIcon,
  HardDriveIcon, DashboardSquare02Icon, SquareUnlock01Icon, Moon02Icon, RadioIcon,
  ServerStack01Icon, Settings02Icon, Sun03Icon, ArrowTurnBackwardIcon, Unlink01Icon,
  WifiConnected01Icon, WifiDisconnected01Icon, TestTubeIcon, WorkflowSquare02Icon,
  Share08Icon, GitCompareIcon, PuzzleIcon, PlugSocketIcon, NoteEditIcon, ChartBarLineIcon, GlobalIcon,
  InformationCircleIcon, ArrowUpDownIcon, ArrowLeftRightIcon, Bookmark01Icon, BugIcon,
  UnavailableIcon, Target01Icon, EraserIcon, FileDownloadIcon, FileUploadIcon, File01Icon, FunctionSquareIcon,
  HashIcon, InfinityIcon, KeyboardIcon, Layers01Icon, LeftToRightListBulletIcon, Link01Icon,
  WorkflowSquare01Icon, PackageIcon, PinLocation01Icon, PinOffIcon, FilterHorizontalIcon,
  SquareIcon, CheckmarkSquare01Icon, UnfoldMoreIcon, UnfoldLessIcon, SortingDownIcon,
  SortByDown01Icon, SortingUpIcon, SortByUp01Icon,
  LayoutThreeColumnIcon,
  SidebarLeftIcon, SidebarLeft01Icon,
  TextWrapIcon, LeftToRightListNumberIcon, FileDiffIcon,
} from '@hugeicons/core-free-icons'

/** @type {Record<string, { lucide: any, huge?: any }>} */
export const ICON_MAP = {
  'x':              { lucide: X,             huge: Cancel01Icon },
  'check':          { lucide: Check,         huge: Tick01Icon },
  'check-circle-2': { lucide: CheckCircle2,  huge: CheckmarkCircle02Icon },
  'chevron-down':   { lucide: ChevronDown,   huge: ArrowDown01Icon },
  'chevron-up':     { lucide: ChevronUp,     huge: ArrowUp01Icon },
  'chevron-right':  { lucide: ChevronRight,  huge: ArrowRight01Icon },
  'chevron-left':   { lucide: ChevronLeft,   huge: ArrowLeft01Icon },
  'plus':           { lucide: Plus,          huge: Add01Icon },
  'minus':          { lucide: Minus,         huge: MinusSignIcon },
  'search':         { lucide: Search,        huge: Search01Icon },
  'trash-2':        { lucide: Trash2,        huge: Delete02Icon },
  'log-out':        { lucide: LogOut,        huge: Logout01Icon },
  'table-2':        { lucide: Table2,        huge: GridTableIcon },
  // A view is an eye everywhere in the app - TabBar picks 'eye' for view tabs,
  // so the sidebar has to agree. Plain table glyphs made views indistinguishable
  // from tables at size-3.
  'table-view':     { lucide: Eye,           huge: EyeIcon },
  'refresh-cw':     { lucide: RefreshCw,     huge: RefreshIcon },
  'copy':           { lucide: Copy,          huge: Copy01Icon },
  'copy-plus':      { lucide: CopyPlus,      huge: Copy01Icon },
  'loader-2':       { lucide: Loader2,       huge: Loading03Icon },
  'key-round':      { lucide: KeyRound,      huge: Key01Icon },
  'external-link':  { lucide: ExternalLink,  huge: LinkSquare01Icon },
  'database':       { lucide: Database,      huge: Database01Icon },
  'code-2':         { lucide: Code2,         huge: SourceCodeIcon },
  'terminal':       { lucide: Terminal,      huge: TerminalIcon },
  'bar-chart-2':    { lucide: BarChart2,     huge: ChartLineData02Icon },
  'sparkles':       { lucide: Sparkles,      huge: SparklesIcon },
  'git-branch':     { lucide: GitBranch,     huge: GitBranchIcon },
  'download':       { lucide: Download,      huge: Download04Icon },
  'play':           { lucide: Play,          huge: PlayIcon },
  'eye':            { lucide: Eye,           huge: EyeIcon },
  'eye-off':        { lucide: EyeOff,        huge: EyeOffIcon },
  'bot':            { lucide: Bot,           huge: AiBrain01Icon },
  'alert-triangle': { lucide: AlertTriangle, huge: Alert02Icon },
  'alert-circle':   { lucide: AlertCircle,   huge: AlertCircleIcon },
  'shield-check':   { lucide: ShieldCheck,   huge: Shield01Icon },
  'pencil':         { lucide: Pencil,        huge: PencilEdit01Icon },
  'braces':         { lucide: Braces,        huge: BracketsIcon },
  'lock':           { lucide: Lock,          huge: SquareLock01Icon },
  'layout-template':{ lucide: LayoutTemplate,huge: DashboardSquare01Icon },
  'history':        { lucide: History,       huge: HistoryIcon },
  'arrow-right':    { lucide: ArrowRight,    huge: ArrowRight01Icon },
  'clock':          { lucide: Clock,         huge: Clock01Icon },
  'zap':            { lucide: Zap,           huge: FlashIcon },
  'settings':       { lucide: Settings,      huge: Settings01Icon },
  'maximize-2':     { lucide: Maximize2,     huge: Maximize01Icon },
  'filter':         { lucide: Filter,        huge: FilterIcon },
  'more-horizontal':{ lucide: MoreHorizontal,huge: MoreHorizontalIcon },
  'menu':           { lucide: Menu,          huge: Menu01Icon },
  'folder-open':    { lucide: FolderOpen,    huge: FolderOpenIcon },
  'folder':         { lucide: Folder,        huge: Folder01Icon },
  'folder-plus':    { lucide: FolderPlus,    huge: FolderAddIcon },
  'file-code':      { lucide: FileCode,      huge: FileScriptIcon },
  'archive':        { lucide: Archive,       huge: Archive01Icon },
  'arrow-up-circle':{ lucide: ArrowUpCircle, huge: CircleArrowUp01Icon },
  'chevrons-down':  { lucide: ChevronsDown,  huge: ArrowDownDoubleIcon },
  'chevrons-left':  { lucide: ChevronsLeft,  huge: ArrowLeftDoubleIcon },
  'chevrons-right': { lucide: ChevronsRight, huge: ArrowRightDoubleIcon },
  'chevrons-up':    { lucide: ChevronsUp,    huge: ArrowUpDoubleIcon },
  'cloud':          { lucide: Cloud,         huge: CloudIcon },
  'command':        { lucide: Command,       huge: CommandIcon },
  'hard-drive':     { lucide: HardDrive,     huge: HardDriveIcon },
  'layout-dashboard':{ lucide: LayoutDashboard, huge: DashboardSquare02Icon },
  'lock-open':      { lucide: LockOpen,      huge: SquareUnlock01Icon },
  'moon':           { lucide: Moon,          huge: Moon02Icon },
  'radio':          { lucide: Radio,         huge: RadioIcon },
  'server':         { lucide: Server,        huge: ServerStack01Icon },
  'settings-2':     { lucide: Settings2,     huge: Settings02Icon },
  'sun':            { lucide: Sun,           huge: Sun03Icon },
  'undo-2':         { lucide: Undo2,         huge: ArrowTurnBackwardIcon },
  'unplug':         { lucide: Unplug,        huge: Unlink01Icon },
  'wifi':           { lucide: Wifi,          huge: WifiConnected01Icon },
  'wifi-off':       { lucide: WifiOff,       huge: WifiDisconnected01Icon },
  'arrow-left':     { lucide: ArrowLeft,     huge: ArrowLeft01Icon },
  'flask-conical':  { lucide: FlaskConical,  huge: TestTubeIcon },
  'workflow':       { lucide: Workflow,      huge: WorkflowSquare02Icon },
  'share-2':        { lucide: Share2,        huge: Share08Icon },
  'git-compare':    { lucide: GitCompare,    huge: GitCompareIcon },
  'blocks':         { lucide: Blocks,        huge: PuzzleIcon },
  'globe':          { lucide: Globe,         huge: GlobalIcon },
  'plug':           { lucide: Plug,          huge: PlugSocketIcon },
  'notebook-pen':   { lucide: NotebookPen,   huge: NoteEditIcon },
  'bar-chart-3':    { lucide: BarChart3,     huge: ChartBarLineIcon },
  'info':           { lucide: Info,          huge: InformationCircleIcon },
  'arrow-down':     { lucide: ArrowDown,      huge: ArrowDown01Icon },
  'arrow-up':       { lucide: ArrowUp,        huge: ArrowUp01Icon },
  'arrow-up-down':  { lucide: ArrowUpDown,    huge: ArrowUpDownIcon },
  'arrow-left-right':{ lucide: ArrowLeftRight,huge: ArrowLeftRightIcon },
  'arrow-down-to-line':{ lucide: ArrowDownToLine, huge: Download04Icon },
  'arrow-left-to-line':{ lucide: ArrowLeftToLine, huge: ArrowLeftDoubleIcon },
  'arrow-right-to-line':{ lucide: ArrowRightToLine, huge: ArrowRightDoubleIcon },
  'replace':        { lucide: ReplaceLucide },
  'bookmark':       { lucide: Bookmark,       huge: Bookmark01Icon },
  'box':            { lucide: Box,            huge: PackageIcon },
  'bug':            { lucide: Bug,            huge: BugIcon },
  'circle-slash':   { lucide: CircleSlash,    huge: UnavailableIcon },
  'clipboard-copy': { lucide: ClipboardCopy,  huge: Copy01Icon },
  'cog':            { lucide: Cog,            huge: Settings01Icon },
  'columns-3':      { lucide: Columns3,       huge: LayoutThreeColumnIcon },
  'crosshair':      { lucide: Crosshair,      huge: Target01Icon },
  'eraser':         { lucide: Eraser,         huge: EraserIcon },
  'file-down':      { lucide: FileDown,       huge: FileDownloadIcon },
  'file-up':        { lucide: FileUp,         huge: FileUploadIcon },
  'file-text':      { lucide: FileText,       huge: File01Icon },
  'function-square':{ lucide: FunctionSquare, huge: FunctionSquareIcon },
  'hash':           { lucide: Hash,           huge: HashIcon },
  'infinity':       { lucide: Infinity,       huge: InfinityIcon },
  'keyboard':       { lucide: Keyboard,       huge: KeyboardIcon },
  'layers':         { lucide: Layers,         huge: Layers01Icon },
  'layout-list':    { lucide: LayoutList,     huge: LeftToRightListBulletIcon },
  'panel-left-close':{ lucide: PanelLeftClose, huge: SidebarLeftIcon },
  'panel-left-open': { lucide: PanelLeftOpen,  huge: SidebarLeft01Icon },
  'link-2':         { lucide: Link2,          huge: Link01Icon },
  'list-filter':    { lucide: ListFilter,     huge: FilterIcon },
  'network':        { lucide: Network,        huge: WorkflowSquare01Icon },
  'package':        { lucide: Package,        huge: PackageIcon },
  'pin':            { lucide: Pin,            huge: PinLocation01Icon },
  'pin-off':        { lucide: PinOff,         huge: PinOffIcon },
  'rotate-ccw':     { lucide: RotateCcw,      huge: ArrowTurnBackwardIcon },
  'sliders-horizontal':{ lucide: SlidersHorizontal, huge: FilterHorizontalIcon },
  'square':         { lucide: Square,         huge: SquareIcon },
  'square-check':   { lucide: SquareCheck,    huge: CheckmarkSquare01Icon },
  'chevrons-up-down':{ lucide: ChevronsUpDown,huge: UnfoldMoreIcon },
  'chevrons-down-up':{ lucide: ChevronsDownUp,huge: UnfoldLessIcon },
  'arrow-down-0-1': { lucide: ArrowDown01,    huge: SortingDownIcon },
  'arrow-down-a-z': { lucide: ArrowDownAZ,    huge: SortByDown01Icon },
  'arrow-up-0-1':   { lucide: ArrowUp01,      huge: SortingUpIcon },
  'arrow-up-a-z':   { lucide: ArrowUpAZ,      huge: SortByUp01Icon },
  'wrap-text':      { lucide: WrapText,       huge: TextWrapIcon },
  'list-ordered':   { lucide: ListOrdered,    huge: LeftToRightListNumberIcon },
  'file-diff':      { lucide: FileDiff,       huge: FileDiffIcon },
}
