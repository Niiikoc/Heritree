import { type FormEvent, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Answers, AskOptions } from "../ui";

interface Props extends AskOptions {
  onDone: (answers: Answers | null) => void;
}

/** A modal that asks for a few values or a confirmation. */
export function Dialog({ title, message, command, fields = [], confirm, danger, onDone }: Props) {
  const [values, setValues] = useState<Answers>(() =>
    Object.fromEntries(fields.map((f) => [f.name, f.checkbox ? !!f.checked : (f.value ?? "")])),
  );
  const set = (name: string, value: string | boolean) => setValues((v) => ({ ...v, [name]: value }));
  const missing = fields.some((f) => !f.checkbox && !f.optional && !String(values[f.name]).trim());

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!missing) onDone(values);
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onDone(null)}>
      <form
        className="modal"
        role="dialog"
        aria-label={title}
        onSubmit={submit}
        onKeyDown={(e) => {
          if (e.key === "Escape") onDone(null);
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) submit(e);
        }}
      >
        <h2>{title}</h2>
        {message && <p className="modal-message">{message}</p>}
        {fields.map((f, i) =>
          f.checkbox ? (
            <label key={f.name} className="checkbox">
              <input type="checkbox" checked={!!values[f.name]} onChange={(e) => set(f.name, e.target.checked)} />
              {f.label}
            </label>
          ) : (
            <label key={f.name} className="field">
              <span>{f.label}</span>
              <span className="field-row">
                {f.multiline ? (
                  <textarea
                    autoFocus={i === 0}
                    rows={4}
                    value={String(values[f.name])}
                    placeholder={f.placeholder}
                    onChange={(e) => set(f.name, e.target.value)}
                  />
                ) : (
                  <input
                    autoFocus={i === 0}
                    value={String(values[f.name])}
                    placeholder={f.placeholder}
                    spellCheck={false}
                    onChange={(e) => set(f.name, e.target.value)}
                  />
                )}
                {f.browse && (
                  <button
                    type="button"
                    onClick={async () => {
                      const picked = await open({ directory: true, title: f.label });
                      if (typeof picked === "string") set(f.name, picked);
                    }}
                  >
                    Browse…
                  </button>
                )}
              </span>
            </label>
          ),
        )}
        {command && (
          <pre className="modal-command" title="The git command this runs">
            {command}
          </pre>
        )}
        <div className="modal-actions">
          <button type="button" onClick={() => onDone(null)}>
            Cancel
          </button>
          <button type="submit" className={danger ? "danger-button" : "primary"} disabled={missing} autoFocus={!fields.length}>
            {confirm}
          </button>
        </div>
      </form>
    </div>
  );
}
