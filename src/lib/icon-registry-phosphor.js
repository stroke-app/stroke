/**
 * The Phosphor icon set: ~110 components, about 300 KB of source, so it is
 * kept out of the startup bundle and loaded only once Phosphor is the chosen
 * set (icon-family.svelte.js). Until it lands, <Icon> draws the Hugeicons or
 * Lucide glyph for the same name.
 */

// ── Phosphor set (third family) ──────────────────────────────────────────────
// Same incremental-coverage contract as Hugeicons: names missing here fall back
// to Lucide in <Icon>, so mappings can grow one at a time without breaking UI.
// Per-file imports keep the bundle to only the icons actually mapped.
import PhX from 'phosphor-svelte/lib/X'
import PhCheck from 'phosphor-svelte/lib/Check'
import PhCheckCircle from 'phosphor-svelte/lib/CheckCircle'
import PhCaretDown from 'phosphor-svelte/lib/CaretDown'
import PhCaretUp from 'phosphor-svelte/lib/CaretUp'
import PhCaretRight from 'phosphor-svelte/lib/CaretRight'
import PhCaretLeft from 'phosphor-svelte/lib/CaretLeft'
import PhPlus from 'phosphor-svelte/lib/Plus'
import PhMinus from 'phosphor-svelte/lib/Minus'
import PhMagnifyingGlass from 'phosphor-svelte/lib/MagnifyingGlass'
import PhTrash from 'phosphor-svelte/lib/Trash'
import PhSignOut from 'phosphor-svelte/lib/SignOut'
import PhTable from 'phosphor-svelte/lib/Table'
import PhArrowsClockwise from 'phosphor-svelte/lib/ArrowsClockwise'
import PhCopy from 'phosphor-svelte/lib/Copy'
import PhCircleNotch from 'phosphor-svelte/lib/CircleNotch'
import PhKey from 'phosphor-svelte/lib/Key'
import PhArrowSquareOut from 'phosphor-svelte/lib/ArrowSquareOut'
import PhDatabase from 'phosphor-svelte/lib/Database'
import PhCode from 'phosphor-svelte/lib/Code'
import PhTerminal from 'phosphor-svelte/lib/Terminal'
import PhChartBar from 'phosphor-svelte/lib/ChartBar'
import PhSparkle from 'phosphor-svelte/lib/Sparkle'
import PhGitBranch from 'phosphor-svelte/lib/GitBranch'
import PhDownloadSimple from 'phosphor-svelte/lib/DownloadSimple'
import PhPlay from 'phosphor-svelte/lib/Play'
import PhEye from 'phosphor-svelte/lib/Eye'
import PhEyeSlash from 'phosphor-svelte/lib/EyeSlash'
import PhRobot from 'phosphor-svelte/lib/Robot'
import PhWarning from 'phosphor-svelte/lib/Warning'
import PhWarningCircle from 'phosphor-svelte/lib/WarningCircle'
import PhShieldCheck from 'phosphor-svelte/lib/ShieldCheck'
import PhGlobe from 'phosphor-svelte/lib/Globe'
import PhPencilSimple from 'phosphor-svelte/lib/PencilSimple'
import PhBracketsCurly from 'phosphor-svelte/lib/BracketsCurly'
import PhLock from 'phosphor-svelte/lib/Lock'
import PhClockCounterClockwise from 'phosphor-svelte/lib/ClockCounterClockwise'
import PhArrowRight from 'phosphor-svelte/lib/ArrowRight'
import PhArrowLeft from 'phosphor-svelte/lib/ArrowLeft'
import PhClock from 'phosphor-svelte/lib/Clock'
import PhLightning from 'phosphor-svelte/lib/Lightning'
import PhGear from 'phosphor-svelte/lib/Gear'
import PhFunnel from 'phosphor-svelte/lib/Funnel'
import PhDotsThree from 'phosphor-svelte/lib/DotsThree'
import PhList from 'phosphor-svelte/lib/List'
import PhFolderOpen from 'phosphor-svelte/lib/FolderOpen'
import PhFolder from 'phosphor-svelte/lib/Folder'
import PhFolderPlus from 'phosphor-svelte/lib/FolderPlus'
import PhFileSql from 'phosphor-svelte/lib/FileSql'
import PhArchive from 'phosphor-svelte/lib/Archive'
import PhCloud from 'phosphor-svelte/lib/Cloud'
import PhCommand from 'phosphor-svelte/lib/Command'
import PhHardDrives from 'phosphor-svelte/lib/HardDrives'
import PhMoon from 'phosphor-svelte/lib/Moon'
import PhSun from 'phosphor-svelte/lib/Sun'
import PhInfo from 'phosphor-svelte/lib/Info'
import PhArrowDown from 'phosphor-svelte/lib/ArrowDown'
import PhArrowUp from 'phosphor-svelte/lib/ArrowUp'
import PhArrowsDownUp from 'phosphor-svelte/lib/ArrowsDownUp'
import PhBookmark from 'phosphor-svelte/lib/Bookmark'
import PhBug from 'phosphor-svelte/lib/Bug'
import PhEraser from 'phosphor-svelte/lib/Eraser'
import PhFileText from 'phosphor-svelte/lib/FileText'
import PhHash from 'phosphor-svelte/lib/Hash'
import PhKeyboard from 'phosphor-svelte/lib/Keyboard'
import PhLink from 'phosphor-svelte/lib/Link'
import PhPushPin from 'phosphor-svelte/lib/PushPin'
import PhPushPinSlash from 'phosphor-svelte/lib/PushPinSlash'
import PhArrowCounterClockwise from 'phosphor-svelte/lib/ArrowCounterClockwise'
import PhCaretUpDown from 'phosphor-svelte/lib/CaretUpDown'
import PhPackage from 'phosphor-svelte/lib/Package'
import PhSquare from 'phosphor-svelte/lib/Square'
import PhCheckSquare from 'phosphor-svelte/lib/CheckSquare'
import PhCaretDoubleDown from 'phosphor-svelte/lib/CaretDoubleDown'
import PhCaretDoubleUp from 'phosphor-svelte/lib/CaretDoubleUp'
import PhCaretDoubleLeft from 'phosphor-svelte/lib/CaretDoubleLeft'
import PhCaretDoubleRight from 'phosphor-svelte/lib/CaretDoubleRight'
import PhFadersHorizontal from 'phosphor-svelte/lib/FadersHorizontal'
import PhSquaresFour from 'phosphor-svelte/lib/SquaresFour'
import PhStack from 'phosphor-svelte/lib/Stack'
import PhPuzzlePiece from 'phosphor-svelte/lib/PuzzlePiece'
import PhWifiHigh from 'phosphor-svelte/lib/WifiHigh'
import PhWifiSlash from 'phosphor-svelte/lib/WifiSlash'
import PhColumns from 'phosphor-svelte/lib/Columns'
import PhSidebar from 'phosphor-svelte/lib/Sidebar'
import PhSidebarSimple from 'phosphor-svelte/lib/SidebarSimple'
import PhPlug from 'phosphor-svelte/lib/Plug'

/** @type {Record<string, any>} */
export const PHOSPHOR_MAP = {
  'x': PhX,
  'check': PhCheck,
  'check-circle-2': PhCheckCircle,
  'chevron-down': PhCaretDown,
  'chevron-up': PhCaretUp,
  'chevron-right': PhCaretRight,
  'chevron-left': PhCaretLeft,
  'plus': PhPlus,
  'minus': PhMinus,
  'search': PhMagnifyingGlass,
  'trash-2': PhTrash,
  'log-out': PhSignOut,
  'table-2': PhTable,
  'table-view': PhEye,
  'refresh-cw': PhArrowsClockwise,
  'copy': PhCopy,
  'copy-plus': PhCopy,
  'loader-2': PhCircleNotch,
  'key-round': PhKey,
  'external-link': PhArrowSquareOut,
  'database': PhDatabase,
  'code-2': PhCode,
  'terminal': PhTerminal,
  'bar-chart-2': PhChartBar,
  'bar-chart-3': PhChartBar,
  'sparkles': PhSparkle,
  'git-branch': PhGitBranch,
  'download': PhDownloadSimple,
  'play': PhPlay,
  'eye': PhEye,
  'eye-off': PhEyeSlash,
  'bot': PhRobot,
  'alert-triangle': PhWarning,
  'alert-circle': PhWarningCircle,
  'shield-check': PhShieldCheck,
  'pencil': PhPencilSimple,
  'braces': PhBracketsCurly,
  'lock': PhLock,
  'history': PhClockCounterClockwise,
  'arrow-right': PhArrowRight,
  'arrow-left': PhArrowLeft,
  'clock': PhClock,
  'zap': PhLightning,
  'settings': PhGear,
  'cog': PhGear,
  'filter': PhFunnel,
  'list-filter': PhFunnel,
  'more-horizontal': PhDotsThree,
  'menu': PhList,
  'folder-open': PhFolderOpen,
  'folder': PhFolder,
  'folder-plus': PhFolderPlus,
  'file-code': PhFileSql,
  'archive': PhArchive,
  'cloud': PhCloud,
  'command': PhCommand,
  'hard-drive': PhHardDrives,
  'moon': PhMoon,
  'sun': PhSun,
  'info': PhInfo,
  'arrow-down': PhArrowDown,
  'arrow-up': PhArrowUp,
  'arrow-up-down': PhArrowsDownUp,
  'bookmark': PhBookmark,
  'bug': PhBug,
  'eraser': PhEraser,
  'file-text': PhFileText,
  'hash': PhHash,
  'keyboard': PhKeyboard,
  'link-2': PhLink,
  'pin': PhPushPin,
  'pin-off': PhPushPinSlash,
  'rotate-ccw': PhArrowCounterClockwise,
  'chevrons-up-down': PhCaretUpDown,
  'package': PhPackage,
  'square': PhSquare,
  'square-check': PhCheckSquare,
  'chevrons-down': PhCaretDoubleDown,
  'chevrons-up': PhCaretDoubleUp,
  'chevrons-left': PhCaretDoubleLeft,
  'chevrons-right': PhCaretDoubleRight,
  'sliders-horizontal': PhFadersHorizontal,
  'settings-2': PhFadersHorizontal,
  'layout-dashboard': PhSquaresFour,
  'panel-left-close': PhSidebarSimple,
  'panel-left-open': PhSidebar,
  'layers': PhStack,
  'blocks': PhPuzzlePiece,
  'globe': PhGlobe,
  'wifi': PhWifiHigh,
  'wifi-off': PhWifiSlash,
  'columns-3': PhColumns,
  'plug': PhPlug,
}
