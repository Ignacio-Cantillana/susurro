import { useCallback, useEffect, useState } from "react";
import { api, useEvent, type Meeting, type MeetingStatus, type Segment } from "../lib/api";
import { formatClock, formatDateTime } from "../lib/format";
import { CopyIcon, RecordIcon, StopIcon, TrashIcon } from "../components/icons";
import { useToast } from "../components/Toast";

export default function Meetings() {
  const toast = useToast();
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [status, setStatus] = useState<MeetingStatus | null>(null);

  const load = useCallback(async () => {
    const list = await api.listMeetings();
    setMeetings(list);
    setSelected((cur) => cur ?? list[0]?.id ?? null);
  }, []);
  useEffect(() => {
    load();
    api.getStatus().then((s) => setStatus(s.meeting));
  }, [load]);
  useEvent<MeetingStatus>("meeting-status", (s) => {
    setStatus((prev) => {
      if (prev?.phase === "idle" && s.phase === "recording") {
        load();
        setSelected(s.id);
      }
      return s;
    });
  }, [load]);
  useEvent("meeting-updated", load, [load]);

  const live = status && status.phase !== "idle";

  return (
    <div className="page" style={{ maxWidth: 1040 }}>
      <div className="page-head">
        <div>
          <h1>Reuniones</h1>
          <p>Graba tu micro («Yo») y el audio del Mac («Otros»): Meet, Zoom, Teams… Se transcribe mientras hablas.</p>
        </div>
        {live ? (
          <button className="btn rec" onClick={() => api.stopMeeting()} disabled={status.phase === "finishing"}>
            <StopIcon /> {status.phase === "finishing" ? "Terminando…" : `Parar · ${formatClock(status.elapsed_ms)}`}
          </button>
        ) : (
          <button
            className="btn primary"
            onClick={() => api.startMeeting().catch((e) => toast(String(e)))}
          >
            <RecordIcon size={16} /> Grabar reunión
          </button>
        )}
      </div>

      {live && !status.system_audio && (
        <div className="card pad small">
          Solo se está grabando tu micrófono. Para grabar a los demás, da permiso de <b>Audio del sistema</b> en
          Ajustes → Permisos y vuelve a empezar.
        </div>
      )}

      {meetings.length === 0 ? (
        <div className="card empty">
          <h2>Sin reuniones</h2>
          <p>Pulsa «Grabar reunión» o el botón ⏺ flotante cuando empiece la llamada.</p>
        </div>
      ) : (
        <div className="meetings">
          <div className="card meeting-list">
            {meetings.map((m) => (
              <button
                key={m.id}
                className={`meeting-item ${selected === m.id ? "active" : ""}`}
                onClick={() => setSelected(m.id)}
              >
                <div className="title">{m.title}</div>
                <div className="meta">
                  {formatDateTime(m.started_at)}
                  {m.ended_at ? ` · ${formatClock(m.ended_at - m.started_at)}` : " · en curso"}
                </div>
              </button>
            ))}
          </div>
          {selected !== null && (
            <MeetingDetail
              key={selected}
              id={selected}
              live={status?.id === selected}
              onDeleted={() => {
                setSelected(null);
                load();
              }}
            />
          )}
        </div>
      )}
    </div>
  );
}

function MeetingDetail({ id, live, onDeleted }: { id: number; live: boolean; onDeleted: () => void }) {
  const toast = useToast();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [turns, setTurns] = useState<Segment[]>([]);
  const [title, setTitle] = useState("");

  const load = useCallback(async () => {
    const d = await api.getMeeting(id);
    if (!d) return;
    setMeeting(d.meeting);
    setTurns(d.turns);
    setTitle((t) => t || d.meeting.title);
  }, [id]);
  useEffect(() => {
    load();
  }, [load]);
  useEvent<number>("meeting-updated", (mid) => mid === id && load(), [id, load]);

  if (!meeting) return null;

  return (
    <div className="card pad section">
      <div className="row-flex" style={{ justifyContent: "space-between", flexWrap: "nowrap" }}>
        <input
          className="title-input"
          value={title}
          aria-label="Título de la reunión"
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => title.trim() && title !== meeting.title && api.renameMeeting(id, title.trim())}
          onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        />
        <div className="row-flex" style={{ flexWrap: "nowrap" }}>
          <button
            className="btn"
            onClick={async () => {
              await api.copy(await api.meetingMarkdown(id));
              toast("Transcripción copiada en Markdown");
            }}
            disabled={turns.length === 0}
          >
            <CopyIcon /> Copiar
          </button>
          <button
            className="icon-btn danger"
            aria-label="Borrar reunión"
            disabled={live}
            onClick={async () => {
              await api.deleteMeeting(id);
              onDeleted();
            }}
          >
            <TrashIcon />
          </button>
        </div>
      </div>
      <div className="row-flex small muted">
        <span>{formatDateTime(meeting.started_at)}</span>
        {meeting.ended_at && <span>· {formatClock(meeting.ended_at - meeting.started_at)}</span>}
        <span className="chip" style={{ color: "var(--me)" }}>Yo</span>
        <span className="chip" style={{ color: "var(--others)" }}>Otros</span>
      </div>
      {live && (
        <div className="live card">
          <span className="dot" />
          <span className="small">Grabando. Cada ~30 s aparece aquí lo nuevo.</span>
        </div>
      )}
      {turns.length === 0 ? (
        <p className="muted">{live ? "Esperando el primer bloque…" : "No se transcribió nada en esta reunión."}</p>
      ) : (
        <div className="transcript">
          {turns.map((t, i) => (
            <div key={i} className={`turn ${t.speaker}`}>
              <div className="who">
                {t.speaker === "me" ? "Yo" : "Otros"}
                <small>{formatClock(t.start_ms)}</small>
              </div>
              <div>{t.text}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
