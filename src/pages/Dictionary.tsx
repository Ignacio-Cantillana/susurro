import { useCallback, useEffect, useState } from "react";
import { api, type DictTerm } from "../lib/api";
import { TrashIcon } from "../components/icons";
import { useToast } from "../components/Toast";

export default function Dictionary() {
  const toast = useToast();
  const [terms, setTerms] = useState<DictTerm[]>([]);
  const [term, setTerm] = useState("");
  const [soundsLike, setSoundsLike] = useState("");

  const load = useCallback(() => api.listDictionary().then(setTerms), []);
  useEffect(() => {
    load();
  }, [load]);

  async function add(e: React.FormEvent) {
    e.preventDefault();
    try {
      await api.upsertDictionary(term, soundsLike);
      setTerm("");
      setSoundsLike("");
      load();
    } catch (err) {
      toast(String(err));
    }
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Diccionario</h1>
          <p>
            Nombres propios y jerga que Whisper suele escribir mal. Se le pasan como contexto y, si aun así se
            equivoca, se corrigen las variantes que pongas.
          </p>
        </div>
      </div>
      <form className="card pad form-row" onSubmit={add}>
        <input className="input" placeholder="Palabra correcta (p. ej. Tauri)" value={term} onChange={(e) => setTerm(e.target.value)} aria-label="Palabra" />
        <input
          className="input"
          placeholder="Cómo lo oye, separado por comas (p. ej. tori, tauri)"
          value={soundsLike}
          onChange={(e) => setSoundsLike(e.target.value)}
          aria-label="Variantes"
        />
        <button className="btn primary" disabled={!term.trim()}>
          Añadir
        </button>
      </form>
      {terms.length === 0 ? (
        <div className="card empty">Aún no hay palabras.</div>
      ) : (
        <div className="card list">
          {terms.map((t) => (
            <div className="row" key={t.id} style={{ gridTemplateColumns: "1fr 1fr auto" }}>
              <div style={{ fontWeight: 600 }}>{t.term}</div>
              <div className="muted">{t.sounds_like || "—"}</div>
              <button className="icon-btn danger" aria-label={`Borrar ${t.term}`} onClick={() => api.deleteDictionary(t.id).then(load)}>
                <TrashIcon />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
