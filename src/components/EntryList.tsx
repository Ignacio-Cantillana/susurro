import { useState } from "react";
import { api, type Entry } from "../lib/api";
import { countWords, formatTime, groupByDay } from "../lib/format";
import { CheckIcon, CopyIcon, TrashIcon } from "./icons";
import { useToast } from "./Toast";

/** Lista de dictados o notas agrupada por día, con copiar, editar y borrar. */
export default function EntryList({ entries, onChange }: { entries: Entry[]; onChange: () => void }) {
  return (
    <>
      {groupByDay(entries, (e) => e.created_at).map(([day, items]) => (
        <section key={day}>
          <div className="day">{day}</div>
          <div className="card list">
            {items.map((e) => (
              <EntryRow key={e.id} entry={e} onChange={onChange} />
            ))}
          </div>
        </section>
      ))}
    </>
  );
}

function EntryRow({ entry, onChange }: { entry: Entry; onChange: () => void }) {
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(entry.text);

  async function save() {
    if (draft.trim() && draft !== entry.text) await api.updateEntry(entry.id, draft);
    setEditing(false);
    onChange();
  }

  const meta = [entry.app, `${countWords(entry.text)} palabras`, entry.raw_text !== entry.text && "corregido"]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="row">
      <div className="time">{formatTime(entry.created_at)}</div>
      <div>
        {editing ? (
          <textarea
            className="textarea"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={save}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) save();
              if (e.key === "Escape") {
                setDraft(entry.text);
                setEditing(false);
              }
            }}
          />
        ) : (
          <div className="text" onDoubleClick={() => setEditing(true)} title="Doble clic para editar">
            {entry.text}
          </div>
        )}
        <div className="meta">{meta}</div>
      </div>
      <div className="actions">
        {editing ? (
          <button className="icon-btn" onClick={save} aria-label="Guardar">
            <CheckIcon />
          </button>
        ) : (
          <button
            className="icon-btn"
            aria-label="Copiar"
            onClick={async () => {
              await api.copy(entry.text);
              toast("Copiado");
            }}
          >
            <CopyIcon />
          </button>
        )}
        <button
          className="icon-btn danger"
          aria-label="Borrar"
          onClick={async () => {
            await api.deleteEntry(entry.id);
            onChange();
          }}
        >
          <TrashIcon />
        </button>
      </div>
    </div>
  );
}
