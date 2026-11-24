import { useEffect, useState } from "react";
import { api, type Permissions } from "../lib/api";

type Item = { key: keyof Permissions; kind: string; name: string; why: string };

// Como Wispr Flow: Accesibilidad basta para el tap activo que lee fn y para pegar.
const ACCESSIBILITY: Item = { key: "accessibility", kind: "accessibility", name: "Accesibilidad", why: "Para detectar la tecla de dictado y pegar el texto donde está el cursor." };
const SYSTEM_AUDIO: Item = { key: "system_audio", kind: "system_audio", name: "Audio del sistema", why: "Solo para reuniones: graba lo que suena en el Mac (Meet, Zoom…). No graba la pantalla." };
// Plan B: solo se pide si con Accesibilidad el atajo sigue sin arrancar.
const INPUT: Item = { key: "input_monitoring", kind: "input_monitoring", name: "Monitorización de entrada", why: "Tu Mac no deja leer la tecla solo con Accesibilidad. Concede también este permiso." };

/** El atajo reintenta cada 2 s; damos un margen antes de pedir el permiso extra. */
const HOTKEY_GRACE_MS = 6000;

/** Estado de los permisos de macOS; se refresca solo mientras está en pantalla. */
export default function PermissionList() {
  const [perms, setPerms] = useState<Permissions | null>(null);
  const [axSince, setAxSince] = useState<number | null>(null);
  useEffect(() => {
    if (perms?.accessibility && axSince === null) setAxSince(Date.now());
  }, [perms?.accessibility, axSince]);
  useEffect(() => {
    const tick = () => api.checkPermissions().then(setPerms);
    tick();
    const t = window.setInterval(tick, 1500);
    return () => window.clearInterval(t);
  }, []);
  if (!perms) return null;
  const needsInput =
    !perms.hotkey_active && !perms.input_monitoring && axSince !== null && Date.now() - axSince > HOTKEY_GRACE_MS;
  const items = needsInput ? [ACCESSIBILITY, INPUT, SYSTEM_AUDIO] : [ACCESSIBILITY, SYSTEM_AUDIO];
  return (
    <div className="card">
      {items.map((it, i) => {
        const ok = perms[it.key];
        return (
          <div className="step" key={it.key}>
            <div className={`num ${ok ? "done" : ""}`}>{ok ? "✓" : i + 1}</div>
            <div>
              <div>{it.name}</div>
              <div className="desc">{it.why}</div>
            </div>
            {ok ? (
              <span className="chip ok">Concedido</span>
            ) : (
              <button className="btn" onClick={() => api.requestPermission(it.kind)}>
                Dar permiso
              </button>
            )}
          </div>
        );
      })}
      <div className="step">
        <div className={`num ${perms.hotkey_active ? "done" : ""}`}>{perms.hotkey_active ? "✓" : "!"}</div>
        <div>
          <div>Atajo global</div>
          <div className="desc">
            {perms.hotkey_active
              ? "Funcionando."
              : "Se activa solo unos segundos después de conceder Accesibilidad. Si no, reinicia Susurro."}
          </div>
        </div>
        <span className={`chip ${perms.hotkey_active ? "ok" : "bad"}`}>{perms.hotkey_active ? "Activo" : "Inactivo"}</span>
      </div>
    </div>
  );
}
