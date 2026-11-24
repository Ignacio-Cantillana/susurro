import { useCallback, useEffect, useMemo, useState } from "react";
import { api, useEvent, type Entry, type SearchHit } from "../lib/api";
import { countWords, formatDateTime } from "../lib/format";
import EntryList from "../components/EntryList";
import { SearchIcon } from "../components/icons";

const WEEK_MS = 7 * 24 * 3600 * 1000;
/** Velocidad media escribiendo a teclado, para estimar el tiempo ahorrado. */
const TYPING_WPM = 40;

export default function History({ hotkey, retentionHours }: { hotkey: string; retentionHours: number }) {
  const retention = retentionHours === 0 ? null : retentionHours % 24 === 0 && retentionHours > 24 ? `${retentionHours / 24} días` : `${retentionHours} h`;
  const [entries, setEntries] = useState<Entry[]>([]);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);

  const load = useCallback(() => api.listEntries("dictation", 500).then(setEntries), []);
  useEffect(() => {
    load();
  }, [load]);
  useEvent("entries-changed", load, [load]);

  useEffect(() => {
    if (!query.trim()) return setHits(null);
    const t = window.setTimeout(() => api.search(query).then(setHits), 150);
    return () => window.clearTimeout(t);
  }, [query]);

  const stats = useMemo(() => {
    const since = Date.now() - WEEK_MS;
    const week = entries.filter((e) => e.created_at >= since);
    const words = week.reduce((n, e) => n + countWords(e.text), 0);
    const spokenMin = week.reduce((n, e) => n + e.duration_ms, 0) / 60000;
    const saved = Math.max(0, words / TYPING_WPM - spokenMin);
    return { words, count: week.length, saved: Math.round(saved) };
  }, [entries]);

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Historial</h1>
          <p>
            Lo que has dictado{retention ? ` en las últimas ${retention}` : ""}. Nada sale de tu Mac salvo que elijas una
            API. Las notas de voz están en Notas y no caducan.
          </p>
        </div>
      </div>

      <div className="stats">
        <div className="card stat">
          <div className="value">{stats.words.toLocaleString("es-ES")}</div>
          <div className="name">palabras esta semana</div>
        </div>
        <div className="card stat">
          <div className="value">{stats.count}</div>
          <div className="name">dictados esta semana</div>
        </div>
        <div className="card stat">
          <div className="value">{stats.saved} min</div>
          <div className="name">ahorrados frente a teclear</div>
        </div>
      </div>

      <div className="search">
        <SearchIcon />
        <input
          className="input"
          placeholder="Buscar en dictados, notas y reuniones…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Buscar"
        />
      </div>

      {hits ? (
        <SearchResults hits={hits} />
      ) : entries.length === 0 ? (
        <div className="card empty">
          <h2>Aún no has dictado nada</h2>
          <p>
            Pon el cursor en cualquier campo de texto, mantén <kbd>{hotkey}</kbd>, habla y suelta.
          </p>
        </div>
      ) : (
        <EntryList entries={entries} onChange={load} />
      )}
    </div>
  );
}

const SOURCE_LABEL = { dictation: "Dictado", note: "Nota", segment: "Reunión" } as const;

function SearchResults({ hits }: { hits: SearchHit[] }) {
  if (hits.length === 0) return <div className="card empty">Sin resultados.</div>;
  return (
    <div className="card list">
      {hits.map((h, i) => (
        <div className="row" key={`${h.source}-${h.id}-${i}`}>
          <div className="time">{formatDateTime(h.created_at)}</div>
          <div>
            <div className="text">{highlight(h.snippet)}</div>
            <div className="meta">
              {SOURCE_LABEL[h.source]}
              {h.meeting_title ? ` · ${h.meeting_title}` : ""}
            </div>
          </div>
          <div />
        </div>
      ))}
    </div>
  );
}

/** El backend marca las coincidencias con «»; aquí se pintan con <mark>. */
function highlight(snippet: string) {
  return snippet.split(/(«[^»]*»)/).map((part, i) =>
    part.startsWith("«") ? <mark key={i}>{part.slice(1, -1)}</mark> : <span key={i}>{part}</span>,
  );
}
