// Capa tipada sobre los comandos de Tauri (src-tauri/src/commands.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

export type EntryKind = "dictation" | "note";
export type Speaker = "me" | "others";
export type Mode = "dictation" | "note";
export type Hotkey = "fn" | "right_option" | "right_command";
export type Provider = "local" | "groq" | "openai";

export interface Entry {
  id: number;
  kind: EntryKind;
  text: string;
  raw_text: string;
  app: string | null;
  duration_ms: number;
  created_at: number;
}

export interface Meeting {
  id: number;
  title: string;
  started_at: number;
  ended_at: number | null;
  audio_path: string | null;
  segment_count: number;
}

export interface Segment {
  speaker: Speaker;
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface SearchHit {
  source: "dictation" | "note" | "segment";
  id: number;
  meeting_title: string | null;
  snippet: string;
  created_at: number;
}

export interface DictTerm {
  id: number;
  term: string;
  sounds_like: string;
}

export interface Snippet {
  id: number;
  trigger: string;
  expansion: string;
}

export interface Settings {
  language: string;
  hotkey: Hotkey;
  provider: Provider;
  local_model: string;
  remove_fillers: boolean;
  sounds: boolean;
  show_widget: boolean;
  keep_meeting_audio: boolean;
  input_device: string | null;
  onboarding_done: boolean;
  /** Horas que se guardan los dictados; 0 = siempre. Las notas no caducan. */
  dictation_retention_hours: number;
}

export interface ModelState {
  id: string;
  name: string;
  file: string;
  size_mb: number;
  description: string;
  downloaded: boolean;
}

export interface Permissions {
  accessibility: boolean;
  input_monitoring: boolean;
  system_audio: boolean;
  hotkey_active: boolean;
}

export interface DictationStatus {
  phase: "idle" | "recording" | "transcribing";
  mode: Mode | null;
  locked: boolean;
}

export interface MeetingStatus {
  phase: "idle" | "recording" | "finishing";
  id: number | null;
  elapsed_ms: number;
  system_audio: boolean;
}

export interface ModelProgress {
  id: string;
  done: number;
  total: number;
  error: string | null;
  finished: boolean;
}

export const api = {
  getStatus: () => invoke<{ dictation: DictationStatus; meeting: MeetingStatus }>("get_status"),
  toggleDictation: (mode: Mode) => invoke<void>("toggle_dictation", { mode }),
  cancelDictation: () => invoke<void>("cancel_dictation"),
  startMeeting: (title?: string) => invoke<number>("start_meeting", { title: title ?? null }),
  stopMeeting: () => invoke<void>("stop_meeting"),

  listEntries: (kind: EntryKind, limit = 50, offset = 0) => invoke<Entry[]>("list_entries", { kind, limit, offset }),
  updateEntry: (id: number, text: string) => invoke<void>("update_entry", { id, text }),
  deleteEntry: (id: number) => invoke<void>("delete_entry", { id }),
  search: (query: string) => invoke<SearchHit[]>("search", { query }),
  copy: (text: string) => invoke<void>("copy_text", { text }),

  listMeetings: (limit = 100, offset = 0) => invoke<Meeting[]>("list_meetings", { limit, offset }),
  getMeeting: (id: number) => invoke<{ meeting: Meeting; turns: Segment[] } | null>("get_meeting", { id }),
  meetingMarkdown: (id: number) => invoke<string>("meeting_markdown", { id }),
  renameMeeting: (id: number, title: string) => invoke<void>("rename_meeting", { id, title }),
  deleteMeeting: (id: number) => invoke<void>("delete_meeting", { id }),

  listDictionary: () => invoke<DictTerm[]>("list_dictionary"),
  upsertDictionary: (term: string, soundsLike: string) =>
    invoke<void>("upsert_dictionary", { term, soundsLike }),
  deleteDictionary: (id: number) => invoke<void>("delete_dictionary", { id }),
  listSnippets: () => invoke<Snippet[]>("list_snippets"),
  upsertSnippet: (trigger: string, expansion: string) => invoke<void>("upsert_snippet", { trigger, expansion }),
  deleteSnippet: (id: number) => invoke<void>("delete_snippet", { id }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  listInputDevices: () => invoke<string[]>("list_input_devices"),
  setApiKey: (provider: string, key: string) => invoke<void>("set_api_key", { provider, key }),
  hasApiKey: (provider: string) => invoke<boolean>("has_api_key", { provider }),
  checkPermissions: () => invoke<Permissions>("check_permissions"),
  requestPermission: (kind: string) => invoke<void>("request_permission", { kind }),

  listModels: () => invoke<ModelState[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),

  showMain: (page?: string) => invoke<void>("show_main", { page: page ?? null }),
  widgetExpand: (mode: "mini" | "normal" | "label") => invoke<void>("widget_expand", { mode }),
  mcpInfo: () => invoke<{ binary: string; command: string; exists: boolean }>("mcp_info"),
};

/** Se suscribe a un evento del backend mientras el componente está montado. */
export function useEvent<T>(name: string, handler: (payload: T) => void, deps: unknown[] = []) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<T>(name, (e) => handler(e.payload)).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name, ...deps]);
}
