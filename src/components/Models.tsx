import { useCallback, useEffect, useState } from "react";
import { api, useEvent, type ModelProgress, type ModelState } from "../lib/api";
import { useToast } from "./Toast";

/** Lista de modelos locales con descarga, progreso y selección. */
export default function Models({ selected, onSelect }: { selected: string; onSelect: (id: string) => void }) {
  const toast = useToast();
  const [models, setModels] = useState<ModelState[]>([]);
  const [progress, setProgress] = useState<Record<string, number>>({});

  const load = useCallback(() => api.listModels().then(setModels), []);
  useEffect(() => {
    load();
  }, [load]);
  useEvent<ModelProgress>("model-progress", (p) => {
    if (p.finished) {
      setProgress(({ [p.id]: _, ...rest }) => rest);
      if (p.error) toast(`Error descargando: ${p.error}`);
      load();
    } else {
      setProgress((prev) => ({ ...prev, [p.id]: p.total ? p.done / p.total : 0 }));
    }
  }, [load]);

  return (
    <div className="card">
      {models.map((m) => {
        const pct = progress[m.id];
        return (
          <div className="model" key={m.id}>
            <input
              type="radio"
              name="model"
              checked={selected === m.id}
              onChange={() => onSelect(m.id)}
              aria-label={m.name}
            />
            <div>
              <div>
                <b>{m.name}</b> <span className="muted small">· {m.size_mb >= 1000 ? `${(m.size_mb / 1000).toFixed(1)} GB` : `${m.size_mb} MB`}</span>
              </div>
              <div className="desc">{m.description}</div>
              {pct !== undefined && (
                <div className="progress" aria-label="Progreso de descarga">
                  <span style={{ width: `${Math.round(pct * 100)}%` }} />
                </div>
              )}
            </div>
            {m.downloaded ? (
              <button className="btn danger" onClick={() => api.deleteModel(m.id).then(load)}>
                Borrar
              </button>
            ) : pct !== undefined ? (
              <span className="small muted">{Math.round(pct * 100)} %</span>
            ) : (
              <button
                className="btn"
                onClick={() => {
                  setProgress((p) => ({ ...p, [m.id]: 0 }));
                  api.downloadModel(m.id);
                }}
              >
                Descargar
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function useModelReady(id: string) {
  const [ready, setReady] = useState(false);
  const check = useCallback(
    () => api.listModels().then((ms) => setReady(ms.some((m) => m.id === id && m.downloaded))),
    [id],
  );
  useEffect(() => {
    check();
  }, [check]);
  useEvent("model-progress", check, [check]);
  return ready;
}
