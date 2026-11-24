import { useCallback, useEffect, useState } from "react";
import { api, type Snippet } from "../lib/api";
import { TrashIcon } from "../components/icons";
import { useToast } from "../components/Toast";

export default function Snippets() {
  const toast = useToast();
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");

  const load = useCallback(() => api.listSnippets().then(setSnippets), []);
  useEffect(() => {
    load();
  }, [load]);

  async function add(e: React.FormEvent) {
    e.preventDefault();
    try {
      await api.upsertSnippet(trigger, expansion);
      setTrigger("");
      setExpansion("");
      load();
    } catch (err) {
      toast(String(err));
    }
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Snippets</h1>
          <p>
            Frases cortas que se convierten en un texto largo. Dicta «mi enlace de calendario» y se pega el enlace
            completo.
          </p>
        </div>
      </div>
      <form className="card pad section" onSubmit={add}>
        <input className="input" placeholder="Lo que dices (p. ej. mi firma)" value={trigger} onChange={(e) => setTrigger(e.target.value)} aria-label="Disparador" />
        <textarea className="textarea" placeholder="Lo que se pega" value={expansion} onChange={(e) => setExpansion(e.target.value)} aria-label="Texto" />
        <div>
          <button className="btn primary" disabled={!trigger.trim() || !expansion}>
            Guardar snippet
          </button>
        </div>
      </form>
      {snippets.length === 0 ? (
        <div className="card empty">Aún no hay snippets.</div>
      ) : (
        <div className="card list">
          {snippets.map((s) => (
            <div className="row" key={s.id} style={{ gridTemplateColumns: "180px 1fr auto" }}>
              <div style={{ fontWeight: 600 }}>«{s.trigger}»</div>
              <div className="text muted">{s.expansion}</div>
              <button className="icon-btn danger" aria-label={`Borrar ${s.trigger}`} onClick={() => api.deleteSnippet(s.id).then(load)}>
                <TrashIcon />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
