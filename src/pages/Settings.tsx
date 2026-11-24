import { useEffect, useState } from "react";
import { api, type Hotkey, type Provider, type Settings } from "../lib/api";
import Models from "../components/Models";
import PermissionList from "../components/PermissionList";
import { useToast } from "../components/Toast";
import { CopyIcon } from "../components/icons";

const LANGUAGES = [
  ["es", "Español"],
  ["en", "Inglés"],
  ["auto", "Detectar automáticamente"],
  ["ca", "Catalán"],
  ["pt", "Portugués"],
  ["fr", "Francés"],
  ["de", "Alemán"],
  ["it", "Italiano"],
] as const;

export default function SettingsPage({ settings, onChange }: { settings: Settings; onChange: () => void }) {
  const toast = useToast();
  const [devices, setDevices] = useState<string[]>([]);
  const [keys, setKeys] = useState<Record<string, boolean>>({});
  const [mcp, setMcp] = useState<{ command: string; exists: boolean } | null>(null);

  useEffect(() => {
    api.listInputDevices().then(setDevices);
    api.mcpInfo().then(setMcp);
    Promise.all(["groq", "openai"].map(async (p) => [p, await api.hasApiKey(p)] as const)).then((r) =>
      setKeys(Object.fromEntries(r)),
    );
  }, []);

  async function update(patch: Partial<Settings>) {
    try {
      await api.saveSettings({ ...settings, ...patch });
      onChange();
    } catch (e) {
      toast(String(e));
    }
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Ajustes</h1>
        </div>
      </div>

      <section className="section">
        <h2>Dictado</h2>
        <div className="card">
          <Field name="Tecla de dictado" desc="Mantenla para hablar; con espacio pasas a manos libres.">
            <Segmented<Hotkey>
              value={settings.hotkey}
              options={[["fn", "fn"], ["right_option", "⌥ dcha"], ["right_command", "⌘ dcha"]]}
              onChange={(hotkey) => update({ hotkey })}
            />
          </Field>
          <Field name="Idioma">
            <select className="select" value={settings.language} onChange={(e) => update({ language: e.target.value })}>
              {LANGUAGES.map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </Field>
          <Field name="Micrófono">
            <select
              className="select"
              value={settings.input_device ?? ""}
              onChange={(e) => update({ input_device: e.target.value || null })}
            >
              <option value="">El del sistema</option>
              {devices.map((d) => (
                <option key={d}>{d}</option>
              ))}
            </select>
          </Field>
          <Field name="Quitar muletillas" desc="«eh», «em», «mmm»…">
            <Switch checked={settings.remove_fillers} onChange={(remove_fillers) => update({ remove_fillers })} />
          </Field>
          <Field name="Sonidos">
            <Switch checked={settings.sounds} onChange={(sounds) => update({ sounds })} />
          </Field>
          <Field name="Guardar dictados" desc="Cuánto tiempo se quedan en el historial. Las notas de voz se guardan siempre.">
            <Segmented<string>
              value={String(settings.dictation_retention_hours)}
              options={[["24", "24 h"], ["168", "7 días"], ["0", "Siempre"]]}
              onChange={(v) => update({ dictation_retention_hours: Number(v) })}
            />
          </Field>
          <Field name="Botones flotantes" desc="Dictar, grabar reunión, nota de voz y abrir el historial, en el borde izquierdo.">
            <Switch checked={settings.show_widget} onChange={(show_widget) => update({ show_widget })} />
          </Field>
        </div>
      </section>

      <section className="section">
        <h2>Motor de transcripción</h2>
        <div className="card">
          <Field name="Dónde se transcribe" desc="En local es gratis, privado y sin límites. Las APIs van más rápido en Macs lentos.">
            <Segmented<Provider>
              value={settings.provider}
              options={[["local", "En el Mac"], ["groq", "Groq"], ["openai", "OpenAI"]]}
              onChange={(provider) => update({ provider })}
            />
          </Field>
          {settings.provider !== "local" && (
            <ApiKeyField
              provider={settings.provider}
              saved={keys[settings.provider]}
              onSaved={(v) => setKeys((k) => ({ ...k, [settings.provider]: v }))}
            />
          )}
        </div>
        {settings.provider === "local" && (
          <Models selected={settings.local_model} onSelect={(local_model) => update({ local_model })} />
        )}
      </section>

      <section className="section">
        <h2>Reuniones</h2>
        <div className="card">
          <Field name="Guardar el audio" desc="Además de la transcripción, guarda dos WAV (tú y los demás).">
            <Switch checked={settings.keep_meeting_audio} onChange={(keep_meeting_audio) => update({ keep_meeting_audio })} />
          </Field>
        </div>
      </section>

      <section className="section">
        <h2>Permisos de macOS</h2>
        <PermissionList />
        <p className="small muted">
          Si la tecla fn abre los emojis o el dictado de Apple: Ajustes del Sistema → Teclado → «Pulsar la tecla 🌐 para» →
          No hacer nada.
        </p>
      </section>

      <section className="section">
        <h2>Claude (MCP)</h2>
        <div className="card pad section">
          <p className="small">
            Susurro incluye un servidor MCP para que Claude lea tus reuniones, dictados y notas (solo lectura). Pega esto en
            una terminal:
          </p>
          {mcp && (
            <div className="row-flex" style={{ flexWrap: "nowrap" }}>
              <div className="code" style={{ flex: 1 }}>
                {mcp.command}
              </div>
              <button
                className="icon-btn"
                aria-label="Copiar comando"
                onClick={async () => {
                  await api.copy(mcp.command);
                  toast("Comando copiado");
                }}
              >
                <CopyIcon />
              </button>
            </div>
          )}
          {mcp && !mcp.exists && (
            <p className="small muted">
              En modo desarrollo compílalo antes con <code>cargo build -p susurro-mcp --release</code> y usa{" "}
              <code>target/release/susurro-mcp</code>.
            </p>
          )}
        </div>
      </section>
    </div>
  );
}

function Field({ name, desc, children }: { name: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <div>
        <div>{name}</div>
        {desc && <div className="desc">{desc}</div>}
      </div>
      {children}
    </div>
  );
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return <button className="switch" role="switch" aria-checked={checked} onClick={() => onChange(!checked)} />;
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented" role="group">
      {options.map(([v, label]) => (
        <button key={v} aria-pressed={v === value} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function ApiKeyField({ provider, saved, onSaved }: { provider: string; saved?: boolean; onSaved: (v: boolean) => void }) {
  const toast = useToast();
  const [key, setKey] = useState("");
  return (
    <Field
      name={`Clave de ${provider === "groq" ? "Groq" : "OpenAI"}`}
      desc={saved ? "Guardada en el Llavero de macOS." : "Se guarda en el Llavero de macOS, nunca en disco."}
    >
      <form
        className="row-flex"
        style={{ flexWrap: "nowrap" }}
        onSubmit={async (e) => {
          e.preventDefault();
          await api.setApiKey(provider, key);
          onSaved(key.trim().length > 0);
          setKey("");
          toast(key.trim() ? "Clave guardada" : "Clave borrada");
        }}
      >
        <input
          className="input"
          type="password"
          placeholder={saved ? "••••••••  (vacío para borrar)" : "Pega tu clave"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          aria-label="Clave de API"
        />
        <button className="btn">Guardar</button>
      </form>
    </Field>
  );
}
