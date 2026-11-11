/**
 * @omfp/kit: thin React Aria Components wrappers styled by token CSS (UI-091, UI-093). Import the styles once:
 * `@omfp/kit/tokens.css` (design tokens, `@layer omfp-tokens`) then `@omfp/kit/kit.css` (`@layer omfp-kit`).
 * Every component forwards `id`, `className`, `data-*` and `aria-*` to its root element.
 */
/** React Aria's unstyled `Autocomplete` (filters a collection from a field; the composer's slash commands use it). */
export { Autocomplete, type AutocompleteProps } from "react-aria-components";
export { Badge, type BadgeProps, Chip, type ChipProps, type Tone } from "./badge.tsx";
export {
  Button,
  type ButtonProps,
  type ButtonSize,
  type ButtonVariant,
  IconButton,
  type IconButtonProps,
} from "./button.tsx";
export { Card, type CardCollapsible, type CardProps, type CardTone } from "./card.tsx";
export {
  ApprovalCommand,
  type ApprovalCommandProps,
  BorderBox,
  type BorderBoxProps,
  type BoxHeadingLevel,
  BoxRule,
  type BoxRuleProps,
  InteractionActions,
  type InteractionActionsProps,
  InteractionCard,
  type InteractionCardProps,
  OptionText,
  type OptionTextProps,
  ResolvedLine,
  type ResolvedLineProps,
  type TodoItem,
  TodoList,
  type TodoListProps,
  ToolCard,
  type ToolCardProps,
  type ToolStatus,
} from "./cards.tsx";
export {
  Checkbox,
  type CheckboxProps,
  Radio,
  RadioGroup,
  type RadioGroupProps,
  type RadioProps,
} from "./choice.tsx";
export { cx } from "./class.ts";
export {
  type Key,
  ListBox,
  ListBoxItem,
  type ListBoxItemProps,
  type ListBoxProps,
  Menu,
  MenuItem,
  type MenuItemProps,
  type MenuProps,
  MenuSection,
  type MenuSectionProps,
  MenuTrigger,
  Select,
  type SelectItem,
  SelectOption,
  type SelectOptionProps,
  type SelectProps,
  Separator,
  type SeparatorProps,
  SubmenuTrigger,
} from "./collection.tsx";
export {
  ConfirmDialog,
  type ConfirmDialogProps,
  Dialog,
  type DialogProps,
  DialogTrigger,
  Popover,
  type PopoverProps,
} from "./dialog.tsx";
export { Diff, type DiffLine, type DiffLineKind, type DiffProps, parsePatch } from "./diff.tsx";
export {
  CodeDiff,
  type CodeDiffLayout,
  type CodeDiffProps,
  CodeEditor,
  type CodeEditorHandle,
  type CodeEditorProps,
  type VimMode,
} from "./editor.tsx";
export {
  EmptyState,
  type EmptyStateProps,
  TOAST_MS,
  Toast,
  type ToastProps,
  ToastRegion,
  type ToastRegionProps,
  type ToastTone,
} from "./feedback.tsx";
export {
  type DataAttributes,
  type FieldDecoration,
  type FieldTexts,
  SearchField,
  type SearchFieldProps,
  TextArea,
  type TextAreaProps,
  TextField,
  type TextFieldProps,
} from "./field.tsx";
export { Highlighted, languageOfPath, loadLanguage } from "./highlight.tsx";
export { Icon, type IconName, type IconProps } from "./icons.tsx";
export { Kbd, type KbdProps } from "./kbd.tsx";
export {
  Disclosure,
  type DisclosureProps,
  Toolbar,
  type ToolbarProps,
  Tooltip,
  type TooltipProps,
  TooltipTrigger,
} from "./layout.tsx";
export {
  AnalyserSource,
  bandEdges,
  type Levels,
  LevelsSource,
  type LiveHint,
  type LiveLineView,
  LivePanel,
  type LivePanelProps,
  type LiveStatus,
  meterValue,
  SILENT_SOURCE,
  SPECTRUM_BARS,
  Spectrum,
  type SpectrumProps,
  type SpectrumSource,
  spectrumFrame,
  Tap,
  VoiceMeter,
} from "./live.tsx";
export { Code, type CodeProps, Markdown, type MarkdownProps } from "./markdown.tsx";
export {
  Count,
  type CountProps,
  compactNumber,
  DiffStat,
  type DiffStatProps,
  ExitCode,
  type ExitCodeProps,
  KindMark,
  type KindMarkProps,
  type MarkState,
  Meter,
  type MeterProps,
  type RepoKind,
  StateBadge,
  type StateBadgeProps,
  StateMark,
  type StateMarkProps,
  type ThreadState,
  useCountUp,
} from "./marks.tsx";
export { type MotionLevel, motionLevel, useMotionLevel } from "./motion.ts";
export { type PathCompletion, PathField, type PathFieldProps, type PathSuggestion } from "./path-field.tsx";
export {
  linkReference,
  type OpenPath,
  PathLink,
  type PathLinkProps,
  type PathReference,
  pathReference,
} from "./paths.tsx";
export {
  ActionRow,
  type ActionRowProps,
  Brand,
  type BrandProps,
  CommandItem,
  type CommandItemProps,
  CommandLine,
  type CommandLineProps,
  CommandMenu,
  type CommandMenuProps,
  HintRow,
  type HintRowProps,
  type Mode,
  ModeLine,
  type ModeLineProps,
  ModePill,
  type ModePillProps,
  ModeSegment,
  type ModeSegmentProps,
  ThreadRow,
  type ThreadRowProps,
  WorkspaceSection,
  type WorkspaceSectionProps,
} from "./shell.tsx";
export {
  LiveRegion,
  type LiveRegionProps,
  Log,
  type LogEntry,
  type LogProps,
  Progress,
  type ProgressProps,
} from "./status.tsx";
export {
  Tab,
  TabList,
  type TabListProps,
  TabPanel,
  type TabPanelProps,
  type TabProps,
  Tabs,
  type TabsProps,
} from "./tabs.tsx";
export {
  ChatHeader,
  type ChatHeaderProps,
  Composer,
  ComposerBar,
  type ComposerBarProps,
  ComposerHint,
  type ComposerHintProps,
  type ComposerProps,
  Message,
  type MessageProps,
  QueueChip,
  type QueueChipProps,
  StreamCaret,
  type StreamCaretProps,
  Thinking,
  type ThinkingProps,
} from "./transcript.tsx";
