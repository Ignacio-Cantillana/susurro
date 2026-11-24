import { useCallback, useEffect, useState } from "react";
import { api, useEvent, type DictationStatus, type Entry } from "../lib/api";
import EntryList from "../components/EntryList";
import { NoteIcon, StopIcon } from "../components/icons";

export default function Notes() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [status, setStatus] = useState<DictationStatus | null>(null);

  const load = useCallback(() => api.listEntries("note", 500).then(setEntries), []);
  useEffect(() => {
    load();
    api.getStatus().then((s) => setStatus(s.dictation));
  }, [load]);
  useEvent("entries-changed", load, [load]);
  useEvent<DictationStatus>("dictation-status", setStatus);

  const recording = status?.phase === "recording" && status.mode === "note";

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Notas</h1>
          <p>Ideas dictadas que se guardan aquí en vez de pegarse. También con el botón 📝 flotante.</p>
        </div>
        <button className={`btn ${recording ? "rec" : "primary"}`} onClick={() => api.toggleDictation("note")}>
          {recording ? <StopIcon /> : <NoteIcon size={16} />}
          {recording ? "Guardar nota" : "Nueva nota de voz"}
        </button>
      </div>
      {entries.length === 0 ? (
        <div className="card empty">
          <h2>Sin notas todavía</h2>
          <p>Pulsa «Nueva nota de voz», habla y vuelve a pulsar para guardarla.</p>
        </div>
      ) : (
        <EntryList entries={entries} onChange={load} />
      )}
    </div>
  );
}
