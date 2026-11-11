/**
 * A path input with suggestions and completion (DMN-021): recently seen repositories first, then the child
 * directories of the typed path on the daemon host. ARIA combobox pattern: the focus stays in the input, the arrows
 * move a highlight through the options (`aria-activedescendant`), Enter takes the highlighted option (else submits),
 * Tab completes the longest common prefix of the directories, Escape closes the list.
 */

import { type KeyboardEvent, type Ref, useEffect, useId, useRef, useState } from "react";
import { type FieldTexts, TextField } from "./field.tsx";
import { Icon } from "./icons.tsx";

/** A repository the daemon has seen (`repos.suggest`). */
export interface PathSuggestion {
  readonly path: string;
  readonly name: string;
}

/** A child directory on the daemon host (`fs.complete`). */
export interface PathCompletion {
  readonly name: string;
  readonly path: string;
  readonly isGitRoot: boolean;
}

export interface PathFieldProps extends FieldTexts {
  value: string;
  onChange: (value: string) => void;
  /** Enter while no option is highlighted (or after accepting one): the caller submits. */
  onSubmit?: (value: string) => void;
  /** Recently seen repositories matching `query` (shown first). Omit for folder-only fields. */
  suggest?: (query: string) => Promise<readonly PathSuggestion[]>;
  /** Child directories for a typed path (called only when the value starts with `/` or `~`). */
  complete: (path: string) => Promise<readonly PathCompletion[]>;
  placeholder?: string;
  autoFocus?: boolean;
  inputRef?: Ref<HTMLInputElement>;
  /** Heading of the suggestions section. */
  suggestionsLabel?: string;
  /** Heading of the directories section. */
  directoriesLabel?: string;
  /** Metadata of directories that are git repositories. */
  gitRootLabel?: string;
  /** Metadata of the other directories. */
  folderLabel?: string;
  /** The key chip at the field's end (the key that completes, `tab` by default; UI-148). */
  kbd?: string | undefined;
  isDisabled?: boolean;
  /** On the <input>. */
  "data-testid"?: string;
}

type PathOption =
  | { readonly kind: "suggestion"; readonly path: string; readonly name: string }
  | { readonly kind: "directory"; readonly path: string; readonly name: string; readonly isGitRoot: boolean };

/** Directory completions and the value they answer (Tab uses them only while that value is still typed). */
interface Completions {
  readonly path: string;
  readonly entries: readonly PathCompletion[];
}

const NO_COMPLETIONS: Completions = { path: "", entries: [] };

/** Values completed on the daemon host: absolute and home-relative paths. */
function isPath(value: string): boolean {
  return value.startsWith("/") || value.startsWith("~");
}

function commonPrefix(names: readonly string[]): string {
  const [first = "", ...rest] = names;
  let end = first.length;
  for (const name of rest) {
    let index = 0;
    while (index < end && name[index] === first[index]) {
      index += 1;
    }
    end = index;
  }
  return first.slice(0, end);
}

/** The value after Tab: a single directory as `path/`, else the typed segment grown to the names' common prefix. */
function tabCompletion(value: string, entries: readonly PathCompletion[]): string {
  const [only, ...others] = entries;
  if (only === undefined) {
    return value;
  }
  if (others.length === 0) {
    return `${only.path}/`;
  }
  const grown = value.slice(0, value.lastIndexOf("/") + 1) + commonPrefix(entries.map((entry) => entry.name));
  return grown.length > value.length && grown.startsWith(value) ? grown : value;
}

function step(active: number | null, count: number, delta: number): number | null {
  if (count === 0) {
    return null;
  }
  if (active === null || active >= count) {
    return delta > 0 ? 0 : count - 1;
  }
  return (active + delta + count) % count;
}

/** A labelled path input with a listbox of suggested repositories and completed directories. */
export function PathField({
  label,
  hideLabel,
  description,
  errorMessage,
  value,
  onChange,
  onSubmit,
  suggest,
  complete,
  placeholder,
  autoFocus,
  inputRef,
  suggestionsLabel = "Recent repositories",
  directoriesLabel = "Folders",
  gitRootLabel = "git",
  folderLabel = "folder",
  kbd = "tab",
  isDisabled,
  "data-testid": testId,
}: PathFieldProps) {
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState<number | null>(null);
  const [suggestions, setSuggestions] = useState<readonly PathSuggestion[]>([]);
  const [completions, setCompletions] = useState<Completions>(NO_COMPLETIONS);
  // The latest sources, so callers need not memoize them and a new closure refetches nothing.
  const sources = useRef({ suggest, complete });
  sources.current = { suggest, complete };

  useEffect(() => {
    if (!open) {
      return undefined;
    }
    // Only the answers set state, never this effect itself: an update from the effect of every key press stays
    // pending after its commit, and a burst of keys (fast typing, WebDriver) then trips React's nested-update limit,
    // which throws in the input's change handler and drops that key. What the effect would reset is derived below.
    // Answers arriving after the value changed (or the list closed) are stale and dropped.
    let current = true;
    const { suggest: fetchSuggestions, complete: fetchCompletions } = sources.current;
    fetchSuggestions?.(value).then(
      (found) => {
        if (current) {
          setSuggestions(found);
          setActive(null);
        }
      },
      // A failed lookup only means nothing to offer; the typed value stays usable.
      () => {
        if (current) setSuggestions([]);
      },
    );
    if (isPath(value)) {
      fetchCompletions(value).then(
        (entries) => {
          if (current) {
            setCompletions({ path: value, entries });
            setActive(null);
          }
        },
        () => {
          if (current) setCompletions({ path: value, entries: [] });
        },
      );
    }
    return () => {
      current = false;
    };
  }, [open, value]);

  const suggested = (suggest === undefined ? [] : suggestions).map(
    (repo): PathOption => ({ kind: "suggestion", path: repo.path, name: repo.name }),
  );
  const directories = (isPath(value) ? completions.entries : []).map(
    (entry): PathOption => ({ kind: "directory", ...entry }),
  );
  const options = [...suggested, ...directories];
  const expanded = open && options.length > 0;
  const highlighted = expanded && active !== null ? options[active] : undefined;
  const optionId = (index: number) => `${listId}-${index}`;

  useEffect(() => {
    if (expanded && active !== null) {
      document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" });
    }
  }, [expanded, active, listId]);

  const change = (next: string) => {
    setOpen(true);
    setActive(null);
    onChange(next);
  };

  const pick = (option: PathOption) => {
    setActive(null);
    if (option.kind === "directory" && !option.isGitRoot) {
      // Keep drilling down: the list stays open on the directory's children.
      setOpen(true);
      onChange(`${option.path}/`);
    } else {
      setOpen(false);
      onChange(option.path);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp": {
        event.preventDefault();
        if (expanded) {
          setActive(step(active, options.length, event.key === "ArrowDown" ? 1 : -1));
        } else {
          setOpen(true);
        }
        return;
      }
      case "Enter": {
        if (highlighted !== undefined) {
          event.preventDefault();
          pick(highlighted);
        } else if (onSubmit !== undefined) {
          event.preventDefault();
          setOpen(false);
          onSubmit(value);
        }
        return;
      }
      case "Tab": {
        if (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) {
          return;
        }
        const next = tabCompletion(value, completions.path === value ? completions.entries : []);
        if (next !== value) {
          event.preventDefault();
          change(next);
        }
        return;
      }
      case "Escape": {
        if (expanded) {
          event.preventDefault();
          event.stopPropagation();
          setOpen(false);
          setActive(null);
        }
        return;
      }
      default:
        return;
    }
  };

  // The typed directory part and the segment being typed: an option shows the first muted and the match bold.
  const typedDirectory = value.slice(0, value.lastIndexOf("/") + 1);
  const typedSegment = value.slice(typedDirectory.length);

  const renderOption = (option: PathOption, index: number) => {
    const gitRoot = option.kind === "directory" && option.isGitRoot;
    const matched = option.kind === "directory" && option.name.startsWith(typedSegment) ? typedSegment.length : 0;
    return (
      // biome-ignore lint/a11y/useFocusableInteractive: options of a combobox stay unfocusable (aria-activedescendant)
      // biome-ignore lint/a11y/useKeyWithClickEvents: the combobox input handles the keys of its options
      <div
        key={`${option.kind}:${option.path}`}
        id={optionId(index)}
        role="option"
        aria-selected={index === active}
        className="omfp-listbox__item omfp-path-field__option"
        data-testid="path-option"
        data-path={option.path}
        data-kind={option.kind}
        data-git-root={gitRoot ? "true" : undefined}
        data-focused={index === active ? "true" : undefined}
        // Keeps the focus in the input, which would otherwise blur and close the list before the click.
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => pick(option)}
      >
        <Icon name={gitRoot || option.kind === "suggestion" ? "folder-git-2" : "folder"} />
        {option.kind === "directory" ? (
          <span className="omfp-path-field__name">
            <span className="omfp-path-field__dir">{typedDirectory}</span>
            {matched > 0 && <b>{option.name.slice(0, matched)}</b>}
            {option.name.slice(matched)}
            {!gitRoot && "/"}
          </span>
        ) : (
          <span className="omfp-path-field__name">{option.name}</span>
        )}
        <span className="omfp-path-field__meta">
          {option.kind === "suggestion" ? option.path : gitRoot ? gitRootLabel : folderLabel}
        </span>
      </div>
    );
  };

  return (
    <div className="omfp-path-field">
      <TextField
        label={label}
        hideLabel={hideLabel === true}
        {...(description === undefined ? {} : { description })}
        {...(errorMessage === undefined ? {} : { errorMessage })}
        {...(placeholder === undefined ? {} : { placeholder })}
        {...(autoFocus === undefined ? {} : { autoFocus })}
        {...(inputRef === undefined ? {} : { inputRef })}
        {...(isDisabled === undefined ? {} : { isDisabled })}
        icon={<Icon name="folder-open" />}
        kbd={kbd}
        mono
        value={value}
        onChange={change}
        inputProps={{
          role: "combobox",
          "aria-autocomplete": "list",
          "aria-expanded": expanded,
          "aria-controls": expanded ? listId : undefined,
          "aria-activedescendant": highlighted === undefined || active === null ? undefined : optionId(active),
          autoComplete: "off",
          autoCorrect: "off",
          spellCheck: false,
          onKeyDown,
          onFocus: () => setOpen(true),
          onBlur: () => {
            setOpen(false);
            setActive(null);
          },
          "data-testid": testId,
        }}
      />
      {expanded && (
        <div
          role="listbox"
          id={listId}
          aria-label={label}
          className="omfp-popover omfp-path-field__list"
          data-testid="path-options"
        >
          {[
            { kind: "suggestion", title: suggestionsLabel, items: suggested, first: 0 },
            { kind: "directory", title: directoriesLabel, items: directories, first: suggested.length },
          ].map(
            ({ kind, title, items, first }) =>
              items.length > 0 && (
                // biome-ignore lint/a11y/useSemanticElements: a group of listbox options, not form fields
                <div key={kind} role="group" aria-label={title}>
                  <div role="presentation" className="omfp-path-field__header">
                    {title}
                  </div>
                  {items.map((option, index) => renderOption(option, first + index))}
                </div>
              ),
          )}
        </div>
      )}
    </div>
  );
}
