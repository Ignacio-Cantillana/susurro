import { useEffect, useRef, useState } from "react";
import { api, useEvent, type DictationStatus, type Hotkey, type MeetingStatus } from "../lib/api";
import { formatClock } from "../lib/format";
import { MicIcon, NoteIcon, RecordIcon, StopIcon, LockIcon, HistoryIcon } from "../components/icons";

const IDLE_DICTATION: DictationStatus = { phase: "idle", mode: null, locked: false };
const IDLE_MEETING: MeetingStatus = { phase: "idle", id: null, elapsed_ms: 0, system_audio: false };
const BARS = 5;
const KEY_LABEL: Record<Hotkey, string> = { fn: "fn", right_option: "⌥ dcha.", right_command: "⌘ dcha." };

type Btn = "mic" | "meeting" | "note" | "history";
type Hint = { text: string; key?: string };

export default function Widget() {
  const [dictation, setDictation] = useState<DictationStatus>(IDLE_DICTATION);
  const [meeting, setMeeting] = useState<MeetingStatus>(IDLE_MEETING);
  const [levels, setLevels] = useState<number[]>(Array(BARS).fill(0));
  // Botón que hay bajo el ratón (lo calculamos con la posición que manda Rust).
  const [hot, setHot] = useState<{ btn: Btn; center: number } | null>(null);
  const hoveredRef = useRef(false);
  const [hotkey, setHotkey] = useState<Hotkey>("fn");
  const [toast, setToast] = useState<{ text: string; tone: "ok" | "error" } | null>(null);
  const toastTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    api.getStatus().then((s) => {
      setDictation(s.dictation);
      setMeeting(s.meeting);
    });
  }, []);

  useEvent<DictationStatus>("dictation-status", (s) => {
    setDictation(s);
    if (s.phase !== "recording") setLevels(Array(BARS).fill(0));
  });
  useEvent<MeetingStatus>("meeting-status", setMeeting);
  useEvent<number>("mic-level", (l) => {
    // Escala perceptual: la voz normal ronda 0.02–0.2 de RMS.
    const v = Math.min(1, Math.sqrt(l) * 2.2);
    setLevels((prev) => [...prev.slice(1), v]);
  });
  useEvent<string>("error", (msg) => showToast(msg, "error"));
  useEvent<string>("warning", (msg) => showToast(msg, "error"));
  useEvent<{ kind: string }>("entries-changed", (e) => {
    if (e.kind === "note") showToast("Nota guardada", "ok");
  });
  useEvent<null>("dictation-empty", () => showToast("No se ha oído nada", "error"));

  function showToast(text: string, tone: "ok" | "error") {
    setToast({ text, tone });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), tone === "error" ? 5000 : 2200);
  }

  const recording = dictation.phase === "recording";
  const dictating = recording && dictation.mode === "dictation";
  const noting = recording && dictation.mode === "note";
  const transcribing = dictation.phase === "transcribing";
  const meetingOn = meeting.phase !== "idle";

  // Sin nada en marcha y sin el ratón encima, se queda en una pestañita en el borde.
  const [hovered, setHovered] = useState(false);
  const leaveTimer = useRef<number | undefined>(undefined);
  const hint = hot ? hintFor(hot.btn) : null;
  const label = toast?.text ?? hint?.text ?? null;
  const busy = recording || transcribing || meetingOn;
  const mini = !hovered && !busy && label === null;

  // El panel solo se ensancha cuando hay algo que enseñar a la derecha de los botones.
  useEffect(() => {
    api.widgetExpand(mini ? "mini" : label !== null ? "label" : "normal");
  }, [mini, label]);

  // Lo avisa Rust (ver `spawn_hover_watch`): solo con pasar el ratón, sin clic. La web
  // no recibe `:hover` en el panel, así que buscamos nosotros qué botón hay en (x, y).
  // Al salir espera medio segundo por si ibas a pulsar algo.
  useEvent<{ inside: boolean; x: number; y: number }>("widget-hover", ({ inside, x, y }) => {
    window.clearTimeout(leaveTimer.current);
    if (inside) {
      if (!hoveredRef.current) api.getSettings().then((s) => setHotkey(s.hotkey)).catch(() => {});
      hoveredRef.current = true;
      setHovered(true);
      const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-btn]");
      const btn = el?.dataset.btn as Btn | undefined;
      setHot((prev) =>
        !btn || !el ? null : prev?.btn === btn ? prev : { btn, center: el.offsetTop + el.offsetHeight / 2 },
      );
    } else {
      setHot(null);
      leaveTimer.current = window.setTimeout(() => {
        hoveredRef.current = false;
        setHovered(false);
      }, 500);
    }
  });

  function hintFor(b: Btn): Hint {
    const key = KEY_LABEL[hotkey];
    switch (b) {
      case "mic":
        if (dictating) return dictation.locked ? { text: "Pulsa para terminar" } : { text: "Suelta", key };
        return { text: "Dictar", key };
      case "meeting":
        return { text: meetingOn ? "Parar reunión" : "Grabar reunión" };
      case "note":
        return { text: noting ? "Pulsa para guardar la nota" : "Nota de voz" };
      case "history":
        return { text: "Historial y notas" };
    }
  }

  if (mini) {
    return (
      <div className="widget mini">
        <div className="handle" />
      </div>
    );
  }

  return (
    <div className="widget">
      <div className="stack">
        <button
          data-btn="mic"
          className={`btn mic ${dictating ? "active" : ""} ${transcribing ? "busy" : ""} ${hot?.btn === "mic" ? "hot" : ""}`}
          onClick={() => api.toggleDictation("dictation")}
          aria-label="Dictar"
        >
          {transcribing ? (
            <span className="spinner" />
          ) : dictating ? (
            <span className="bars">
              {levels.map((v, i) => (
                <span key={i} style={{ height: `${18 + v * 82}%` }} />
              ))}
            </span>
          ) : (
            <MicIcon size={17} />
          )}
          {dictating && dictation.locked && (
            <span className="lock">
              <LockIcon size={9} />
            </span>
          )}
        </button>

        <button
          data-btn="meeting"
          className={`btn round ${meetingOn ? "rec" : ""} ${hot?.btn === "meeting" ? "hot" : ""}`}
          onClick={() => (meetingOn ? api.stopMeeting() : api.startMeeting().catch((e) => showToast(String(e), "error")))}
          aria-label={meetingOn ? "Parar reunión" : "Grabar reunión"}
        >
          {meeting.phase === "finishing" ? (
            <span className="spinner" />
          ) : meetingOn ? (
            <span className="rec-inner">
              <StopIcon size={10} />
              <small>{formatClock(meeting.elapsed_ms)}</small>
            </span>
          ) : (
            <RecordIcon size={15} />
          )}
        </button>

        <button
          data-btn="note"
          className={`btn round ${noting ? "active" : ""} ${hot?.btn === "note" ? "hot" : ""}`}
          onClick={() => api.toggleDictation("note")}
          onContextMenu={(e) => {
            e.preventDefault();
            api.showMain("notes");
          }}
          aria-label="Nota de voz"
        >
          {noting ? <StopIcon size={11} /> : <NoteIcon size={15} />}
        </button>

        <button
          data-btn="history"
          className={`btn round small ${hot?.btn === "history" ? "hot" : ""}`}
          onClick={() => api.showMain("history")}
          aria-label="Abrir Susurro: historial y notas"
        >
          <HistoryIcon size={13} />
        </button>
      </div>
      {label && (
        <div
          className={`label ${toast ? `toast ${toast.tone}` : ""}`}
          role="status"
          // A la altura del botón que señala: su centro menos el padding del panel (4) y media etiqueta (15).
          style={hint && !toast && hot ? { marginTop: Math.max(0, hot.center - 19) } : undefined}
        >
          {hint && !toast ? (
            <>
              {hint.text}
              {hint.key && <b>{hint.key}</b>}
            </>
          ) : (
            label
          )}
        </div>
      )}
    </div>
  );
}
