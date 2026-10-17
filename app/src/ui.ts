// Small shared UI contracts: context menus and dialogs.

export type MenuItem =
  | "separator"
  | {
      label: string;
      onSelect?: () => void;
      danger?: boolean;
      disabled?: boolean;
      /** Shown on the right, e.g. a shortcut. */
      hint?: string;
    };

export interface Field {
  name: string;
  label: string;
  value?: string;
  placeholder?: string;
  /** May be left empty. */
  optional?: boolean;
  multiline?: boolean;
  /** A checkbox instead of a text field. */
  checkbox?: boolean;
  checked?: boolean;
  /** Adds a "Browse…" button that picks a folder. */
  browse?: boolean;
}

export interface AskOptions {
  title: string;
  message?: string;
  /** The git command(s) this will run, shown before destructive actions. */
  command?: string;
  fields?: Field[];
  confirm: string;
  danger?: boolean;
}

export type Answers = Record<string, string | boolean>;

/** True when a key press should go to a text field, not to app shortcuts. */
export function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

/** Runs a repository action: shows progress, reports errors, records undo and refreshes. */
export type Run = (label: string, action: () => Promise<import("./types").Undo | void>) => Promise<boolean>;

/** Opens the confirmation / input dialog; resolves to `null` when cancelled. */
export type Ask = (options: AskOptions) => Promise<Answers | null>;

/** Opens a context menu at the pointer. */
export type OpenMenu = (event: { clientX: number; clientY: number; preventDefault(): void }, items: MenuItem[]) => void;
