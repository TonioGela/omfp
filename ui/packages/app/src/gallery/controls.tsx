/** The gallery's controls (UI-173): buttons, fields, path fields, selects, choices, tabs, menus and dialogs. */
import {
  Button,
  Checkbox,
  ConfirmDialog,
  Icon,
  IconButton,
  ListBox,
  Menu,
  MenuItem,
  PathField,
  Radio,
  RadioGroup,
  Select,
  SelectOption,
  Separator,
  Tab,
  TabList,
  Tabs,
  TextField,
} from "@omfp/kit";
import { useState } from "react";
import { type Forced, forcedClass, LEVELS, Line, MODELS, Section, StateGrid } from "./parts.tsx";

export function Buttons() {
  const variants = [
    { label: "primary", variant: "primary", text: "Send" },
    { label: "secondary", variant: "secondary", text: "Steer" },
    { label: "quiet", variant: "quiet", text: "Cancel" },
    { label: "danger", variant: "danger", text: "Delete", icon: <Icon name="trash-2" /> },
  ] as const;
  return (
    <Section title="Buttons" note="UI-149 · first column is live" wide>
      <StateGrid
        rows={[
          ...variants.map((v) => ({
            label: v.label,
            render: (state: Forced) => (
              <Button
                variant={v.variant}
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
                icon={"icon" in v ? v.icon : undefined}
              >
                {v.text}
              </Button>
            ),
          })),
          {
            label: "icon",
            render: (state: Forced) => (
              <span className="omfp-gallery__line">
                <IconButton
                  label="Refresh"
                  icon={<Icon name="refresh-cw" />}
                  size="md"
                  className={forcedClass(state)}
                  isDisabled={state === "disabled"}
                />
                <IconButton
                  label="Settings"
                  variant="secondary"
                  size="md"
                  icon={<Icon name="settings-2" />}
                  className={forcedClass(state)}
                  isDisabled={state === "disabled"}
                />
              </span>
            ),
          },
        ]}
      />
      <div className="omfp-gallery__divider" />
      <Line label="busy">
        <Button variant="primary" isPending>
          Send
        </Button>
        <Button variant="secondary" isPending icon={<Icon name="refresh-cw" />}>
          Rescan
        </Button>
        <Button variant="danger" size="sm" isPending>
          Remove
        </Button>
      </Line>
      <div className="omfp-gallery__divider" />
      <Line label="small">
        <Button variant="primary" size="sm">
          Small
        </Button>
        <Button variant="secondary" size="sm" icon={<Icon name="plus" />}>
          thread
        </Button>
        <Button variant="quiet" size="sm" kbd="esc">
          Cancel
        </Button>
        <Button variant="danger" size="sm" icon={<Icon name="trash-2" />}>
          Delete
        </Button>
        <IconButton label="Close" icon={<Icon name="x" />} />
        <IconButton label="Settings" variant="secondary" icon={<Icon name="settings-2" />} />
      </Line>
      <Line label="default">
        <Button variant="primary">Medium</Button>
        <Button variant="secondary" icon={<Icon name="plus" />}>
          workspace
        </Button>
        <Button variant="primary" icon={<Icon name="arrow-up" />} kbd="enter">
          Send
        </Button>
        <Button variant="danger" icon={<Icon name="square" />} kbd="esc">
          Stop
        </Button>
        <IconButton label="Close" size="md" icon={<Icon name="x" />} />
        <IconButton label="Settings" size="md" variant="secondary" icon={<Icon name="settings-2" />} />
      </Line>
    </Section>
  );
}

export function Fields() {
  const fieldStates: readonly Forced[] = ["live", "hover", "focus", "disabled"];
  return (
    <Section title="Text fields" note="UI-150" wide>
      <StateGrid
        states={fieldStates}
        rows={[
          {
            label: "filter",
            render: (state) => (
              <TextField
                label="Filter threads"
                hideLabel
                placeholder="Filter threads"
                icon={<Icon name="search" />}
                kbd="/"
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
          {
            label: "plain",
            render: (state) => (
              <TextField
                label="Title"
                hideLabel
                defaultValue={state === "disabled" ? "read only" : "Redesign kit"}
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
          {
            label: "mono",
            render: (state) => (
              <TextField
                label="Command"
                hideLabel
                mono
                defaultValue="bun test --watch"
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
        ]}
      />
      <div className="omfp-gallery__divider" />
      <Line label="invalid">
        <TextField
          label="Folder"
          hideLabel
          mono
          defaultValue="~/projets/pmo"
          errorMessage="No such folder"
          className="omfp-gallery__field"
        />
      </Line>
      <Line label="labelled">
        <TextField
          label="Workspace name"
          description="Letters, digits and dashes."
          defaultValue="dotfiles"
          className="omfp-gallery__field"
        />
      </Line>
    </Section>
  );
}

const FOLDERS = [
  { name: "pmo", git: true, meta: "git · master" },
  { name: "pi-mono", git: true, meta: "git · main" },
  { name: "playground", git: false, meta: "folder" },
  { name: "papers", git: false, meta: "folder" },
] as const;

/** A path field as it looks while completing (UI-151): the kit's PathField markup, open, without focus. */
function StaticCompletion() {
  return (
    <div className="omfp-path-field">
      <TextField
        label="Folder"
        hideLabel
        mono
        icon={<Icon name="folder-open" />}
        kbd="tab"
        value="~/projects/p"
        className="is-focus"
      />
      <div className="omfp-popover omfp-path-field__list omfp-gallery__static" role="listbox" aria-label="Folders">
        {FOLDERS.map((folder, index) => (
          <div
            key={folder.name}
            role="option"
            aria-selected={index === 0}
            tabIndex={-1}
            className="omfp-listbox__item omfp-path-field__option"
          >
            <Icon name={folder.git ? "folder-git-2" : "folder"} />
            <span className="omfp-path-field__name">
              <span className="omfp-path-field__dir">~/projects/</span>
              <b>p</b>
              {folder.name.slice(1)}/
            </span>
            <span className="omfp-path-field__meta">{folder.meta}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

const COMPLETIONS = ["/home/you/projects/pmo", "/home/you/projects/pi-mono", "/home/you/projects/playground"];

export function PathFields() {
  const [value, setValue] = useState("");
  return (
    <Section title="Path field" note="UI-151 · type ~/ · tab completes">
      <Line label="typing">
        <StaticCompletion />
      </Line>
      <Line label="live">
        <PathField
          label="Folder"
          hideLabel
          placeholder="~/projects/…"
          value={value}
          onChange={setValue}
          complete={async (path) =>
            COMPLETIONS.filter((candidate) => candidate.startsWith(path.replace(/^~/, "/home/you"))).map(
              (candidate) => ({
                name: candidate.slice(candidate.lastIndexOf("/") + 1),
                path: candidate,
                isGitRoot: candidate.endsWith("pmo"),
              }),
            )
          }
        />
      </Line>
      <Line label="no match">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/nowhere/x"
        />
      </Line>
      <Line label="invalid">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/projets/pmo"
          errorMessage="No such folder"
        />
      </Line>
      <Line label="disabled">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/projects/pmo"
          isDisabled
        />
      </Line>
    </Section>
  );
}
export function Selects() {
  const [model, setModel] = useState<string>("opus-4.5");
  return (
    <Section title="Select" note="UI-152 · ↓ opens · j/k · enter">
      <Line>
        <Select
          label="Model"
          hideLabel
          icon={<Icon name="brain" />}
          items={MODELS}
          selectedKey={model}
          onSelectionChange={(key) => setModel(String(key))}
        />
        <Select
          label="Thinking"
          hideLabel
          prefix="thinking"
          items={LEVELS}
          selectedKey="high"
          onSelectionChange={() => undefined}
          className="is-hover"
        />
        <Select
          label="Density"
          hideLabel
          items={[
            { id: "compact", label: "compact" },
            { id: "normal", label: "normal" },
          ]}
          selectedKey="compact"
          onSelectionChange={() => undefined}
          className="is-focus"
        />
        <Select
          label="Model"
          hideLabel
          items={MODELS}
          selectedKey="sonnet-4.5"
          onSelectionChange={() => undefined}
          isDisabled
        />
      </Line>
      <div className="omfp-gallery__open">
        <Select
          label="Thinking"
          hideLabel
          prefix="thinking"
          items={LEVELS}
          selectedKey="high"
          onSelectionChange={() => undefined}
          className="is-open"
        />
        <div className="omfp-popover omfp-gallery__static omfp-gallery__options">
          <ListBox aria-label="Thinking" selectionMode="single" defaultSelectedKeys={["high"]} disabledKeys={["xhigh"]}>
            {LEVELS.map((level) => (
              <SelectOption
                key={level.id}
                id={level.id}
                textValue={level.label}
                className={level.id === "medium" ? "is-hover" : ""}
                meta={level.id === "xhigh" ? "unsupported" : undefined}
              >
                {level.id === "high" ? (
                  <>
                    <b>hi</b>gh
                  </>
                ) : (
                  level.label
                )}
              </SelectOption>
            ))}
          </ListBox>
        </div>
      </div>
    </Section>
  );
}

export function Choices() {
  return (
    <Section title="Checkbox · radio" note="UI-153">
      <Line>
        <Checkbox>Unchecked</Checkbox>
        <Checkbox defaultSelected>Checked</Checkbox>
        <Checkbox isIndeterminate>Mixed</Checkbox>
      </Line>
      <Line>
        <Checkbox className="is-hover">Hover</Checkbox>
        <Checkbox defaultSelected className="is-focus">
          Focus
        </Checkbox>
        <Checkbox isDisabled>Disabled</Checkbox>
        <Checkbox isDisabled defaultSelected>
          Disabled
        </Checkbox>
      </Line>
      <RadioGroup
        label="Mode"
        hideLabel
        defaultValue="single"
        orientation="horizontal"
        className="omfp-gallery__radios"
      >
        <Radio value="single">Single</Radio>
        <Radio value="other" className="is-hover">
          Hover
        </Radio>
        <Radio value="focus" className="is-focus">
          Focus
        </Radio>
        <Radio value="disabled" isDisabled>
          Disabled
        </Radio>
      </RadioGroup>
    </Section>
  );
}

export function TabsDemo() {
  return (
    <Section title="Tabs" note="UI-154 · ← → when focused">
      <Tabs defaultSelectedKey="changes">
        <TabList aria-label="Panels">
          <Tab id="workspace">Workspace</Tab>
          <Tab id="changes">Changes</Tab>
          <Tab id="todos">Todos</Tab>
          <Tab id="tree">Tree</Tab>
        </TabList>
      </Tabs>
      <Tabs defaultSelectedKey="changes" disabledKeys={["disabled"]}>
        <TabList aria-label="States">
          <Tab id="changes" className="is-focus">
            Focus
          </Tab>
          <Tab id="hover" className="is-hover">
            Hover
          </Tab>
          <Tab id="disabled">Disabled</Tab>
        </TabList>
      </Tabs>
    </Section>
  );
}

export function MenuDemo() {
  return (
    <Section title="Menu" note="UI-155">
      <div className="omfp-popover omfp-gallery__static omfp-gallery__menu">
        <Menu aria-label="Workspace" disabledKeys={["archive"]}>
          <MenuItem id="rename" icon={<Icon name="pencil" />} kbd="r">
            Rename
          </MenuItem>
          <MenuItem id="copy" icon={<Icon name="copy" />} kbd="y p" className="is-hover">
            Copy path
          </MenuItem>
          <MenuItem id="rescan" icon={<Icon name="refresh-cw" />} kbd=":rescan" className="is-focus">
            Rescan
          </MenuItem>
          <MenuItem id="archive" icon={<Icon name="archive" />}>
            Archive all threads
          </MenuItem>
          <Separator />
          <MenuItem id="remove" icon={<Icon name="trash-2" />} kbd="d d" danger>
            Remove workspace
          </MenuItem>
        </Menu>
      </div>
    </Section>
  );
}

export function Dialogs() {
  const [open, setOpen] = useState(false);
  const message = (
    <>
      Its folder <code>~/.local/share/omfp/ws/dotfiles</code> is deleted. Linked projects are kept.
    </>
  );
  const common = {
    isOpen: true,
    onOpenChange: () => undefined,
    title: "Remove workspace “dotfiles”?",
    message,
    confirmLabel: "Remove",
    tone: "danger",
    cancelKbd: "esc",
    confirmKbd: "enter",
    confirmIcon: <Icon name="trash-2" />,
    onConfirm: () => undefined,
    inline: true,
  } as const;
  return (
    <Section title="Dialog" note="UI-156 · focus trapped · esc closes" full>
      <div className="omfp-gallery__stages">
        <div className="omfp-gallery__stage">
          <ConfirmDialog {...common} testId="dialog-open">
            <Checkbox>Also delete its 2 threads</Checkbox>
          </ConfirmDialog>
        </div>
        <div className="omfp-gallery__stage">
          <ConfirmDialog {...common} testId="dialog-busy" busy />
        </div>
        <div className="omfp-gallery__stage">
          <ConfirmDialog
            {...common}
            testId="dialog-error"
            busy={false}
            error="The folder is in use by a running thread."
          />
        </div>
      </div>
      <Line>
        <Button variant="secondary" size="sm" onPress={() => setOpen(true)}>
          Open as modal
        </Button>
      </Line>
      <ConfirmDialog
        isOpen={open}
        onOpenChange={setOpen}
        title="Remove workspace “dotfiles”?"
        message={message}
        confirmLabel="Remove"
        tone="danger"
        cancelKbd="esc"
        confirmKbd="enter"
        onConfirm={() => undefined}
      />
    </Section>
  );
}
