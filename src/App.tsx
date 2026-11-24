import { useCallback, useEffect, useState } from "react";
import { api, useEvent, type Settings } from "./lib/api";
import { BookIcon, BoltIcon, GearIcon, HistoryIcon, MeetingIcon, NoteIcon } from "./components/icons";
import { ToastProvider } from "./components/Toast";
import History from "./pages/History";
import Notes from "./pages/Notes";
import Meetings from "./pages/Meetings";
import Dictionary from "./pages/Dictionary";
import Snippets from "./pages/Snippets";
import SettingsPage from "./pages/Settings";
import Onboarding from "./pages/Onboarding";

export type Page = "history" | "notes" | "meetings" | "dictionary" | "snippets" | "settings" | "onboarding";

const NAV: { id: Page; label: string; icon: React.ReactNode }[] = [
  { id: "history", label: "Historial", icon: <HistoryIcon /> },
  { id: "notes", label: "Notas", icon: <NoteIcon size={18} /> },
  { id: "meetings", label: "Reuniones", icon: <MeetingIcon /> },
  { id: "dictionary", label: "Diccionario", icon: <BookIcon /> },
  { id: "snippets", label: "Snippets", icon: <BoltIcon /> },
  { id: "settings", label: "Ajustes", icon: <GearIcon /> },
];

const HOTKEY_LABEL: Record<Settings["hotkey"], string> = {
  fn: "fn",
  right_option: "⌥ derecha",
  right_command: "⌘ derecha",
};

export default function App() {
  const [page, setPage] = useState<Page>("history");
  const [settings, setSettings] = useState<Settings | null>(null);

  const reloadSettings = useCallback(() => api.getSettings().then(setSettings), []);
  useEffect(() => {
    reloadSettings().then(() => undefined);
  }, [reloadSettings]);
  useEffect(() => {
    if (settings && !settings.onboarding_done) setPage("onboarding");
  }, [settings?.onboarding_done]); // eslint-disable-line react-hooks/exhaustive-deps

  useEvent<string>("navigate", (p) => setPage(p as Page));

  if (!settings) return null;
  const key = HOTKEY_LABEL[settings.hotkey];

  return (
    <ToastProvider>
      <div className="drag" data-tauri-drag-region />
      {page === "onboarding" ? (
        <main className="main">
          <Onboarding
            settings={settings}
            onDone={async () => {
              await api.saveSettings({ ...settings, onboarding_done: true });
              await reloadSettings();
              setPage("history");
            }}
          />
        </main>
      ) : (
        <div className="app">
          <nav className="sidebar" aria-label="Secciones">
            <div className="brand">
              <img src="/icon.png" alt="" />
              Susurro
            </div>
            {NAV.map((n) => (
              <button
                key={n.id}
                className={`nav ${page === n.id ? "active" : ""}`}
                aria-current={page === n.id ? "page" : undefined}
                onClick={() => setPage(n.id)}
              >
                {n.icon}
                {n.label}
              </button>
            ))}
            <div className="spacer" />
            <div className="hint-card">
              Mantén <kbd>{key}</kbd> para dictar en cualquier app. <kbd>{key}</kbd>+<kbd>espacio</kbd> para manos
              libres; <kbd>esc</kbd> cancela.
            </div>
          </nav>
          <main className="main">
            {page === "history" && <History hotkey={key} retentionHours={settings.dictation_retention_hours} />}
            {page === "notes" && <Notes />}
            {page === "meetings" && <Meetings />}
            {page === "dictionary" && <Dictionary />}
            {page === "snippets" && <Snippets />}
            {page === "settings" && <SettingsPage settings={settings} onChange={reloadSettings} />}
          </main>
        </div>
      )}
    </ToastProvider>
  );
}
