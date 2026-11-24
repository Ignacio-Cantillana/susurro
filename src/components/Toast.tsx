import { createContext, useCallback, useContext, useRef, useState } from "react";
import { useEvent } from "../lib/api";

const ToastCtx = createContext<(msg: string) => void>(() => {});

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [msg, setMsg] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((m: string) => {
    setMsg(m);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 2600);
  }, []);
  // Los errores del backend también se ven en la ventana principal.
  useEvent<string>("error", show, [show]);
  return (
    <ToastCtx.Provider value={show}>
      {children}
      <div className="toast-host" aria-live="polite">
        {msg && <div className="toast">{msg}</div>}
      </div>
    </ToastCtx.Provider>
  );
}

export const useToast = () => useContext(ToastCtx);
