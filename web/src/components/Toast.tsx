import { create } from "zustand";
import { AlertCircle, CheckCircle2, Info } from "lucide-react";

/* --------------------------------------------------------------------------
   Lightweight toast notifications — global, no context/providers needed.
   Mounted once inside <Layout /> via <ToastContainer />.
   -------------------------------------------------------------------------- */

export type ToastType = "success" | "error" | "info";

interface ToastItem {
  id: number;
  type: ToastType;
  message: string;
}

interface ToastStore {
  toasts: ToastItem[];
  push: (item: ToastItem) => void;
  remove: (id: number) => void;
}

const TTL_MS = 5000;
const MAX_TOASTS = 5;

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],
  push: (item) =>
    set((s) => ({
      toasts: [...s.toasts, item].slice(-MAX_TOASTS),
    })),
  remove: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

let seq = 0;

export function toast(message: string, type: ToastType = "error") {
  if (!message) return;
  const id = ++seq;
  useToastStore.getState().push({ id, type, message });
  setTimeout(() => useToastStore.getState().remove(id), TTL_MS);
}

export const toastError = (message: string) => toast(message, "error");
export const toastSuccess = (message: string) => toast(message, "success");
export const toastInfo = (message: string) => toast(message, "info");

export default function ToastContainer() {
  const { toasts, remove } = useToastStore();
  if (toasts.length === 0) return null;

  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast-${t.type}`}>
          <span className="toast-icon" aria-hidden>
            {t.type === "success" ? <CheckCircle2 size={18} /> : t.type === "error" ? <AlertCircle size={18} /> : <Info size={18} />}
          </span>
          <span className="toast-message">{t.message}</span>
          <button
            type="button"
            aria-label="dismiss"
            onClick={() => remove(t.id)}
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}
