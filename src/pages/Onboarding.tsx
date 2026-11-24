import { api, type Settings } from "../lib/api";
import Models, { useModelReady } from "../components/Models";
import PermissionList from "../components/PermissionList";

export default function Onboarding({ settings, onDone }: { settings: Settings; onDone: () => void }) {
  const ready = useModelReady(settings.local_model);
  return (
    <div className="onboarding">
      <div className="hero">
        <img src="/icon.png" alt="" />
        <h1>Bienvenido a Susurro</h1>
        <p>
          Mantén <kbd>fn</kbd>, habla y suelta: el texto aparece donde esté el cursor. Todo se transcribe en tu Mac, sin
          límites ni suscripciones.
        </p>
      </div>

      <section className="section">
        <h2>1. Permisos</h2>
        <PermissionList />
      </section>

      <section className="section">
        <h2>2. Modelo de voz</h2>
        <p className="small muted">Se descarga una vez. El recomendado va bien en cualquier Mac con Apple Silicon.</p>
        <Models
          selected={settings.local_model}
          onSelect={(local_model) => api.saveSettings({ ...settings, local_model })}
        />
      </section>

      <div className="row-flex" style={{ justifyContent: "flex-end" }}>
        <button className="btn" onClick={onDone}>
          Ahora no
        </button>
        <button className="btn primary" onClick={onDone} disabled={!ready}>
          {ready ? "Empezar" : "Descarga un modelo para empezar"}
        </button>
      </div>
    </div>
  );
}
