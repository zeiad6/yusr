import { create } from "zustand";
import type { Role } from "../api/types";
import type { Permission } from "./permissions";

interface Session {
  token: string;
  username: string;
  fullName: string;
  role: Role;
  /** Unix seconds; the server rejects the token after this. */
  expiresAt: number;
  /** Effective permissions, refreshed from /auth/me on every app start. */
  permissions: string[];
}

interface AuthState {
  session: Session | null;
  token: string | null;
  login: (s: Session) => void;
  logout: () => void;
  setPermissions: (p: string[]) => void;
  can: (p: Permission) => boolean;
}

const KEY = "yusr-session";

function stored(): Session | null {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "null") as Session | null;
    return s && s.expiresAt * 1000 > Date.now() ? { ...s, permissions: s.permissions ?? [] } : null;
  } catch {
    return null;
  }
}

const initial = stored();

export const useAuthStore = create<AuthState>()((set, get) => ({
  session: initial,
  token: initial?.token ?? null,
  login: (s) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(s));
    } catch {
      /* private mode: session lives in memory only */
    }
    set({ session: s, token: s.token });
  },
  logout: () => {
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* private mode */
    }
    set({ session: null, token: null });
  },
  setPermissions: (permissions) => {
    const s = get().session;
    if (s) get().login({ ...s, permissions });
  },
  can: (p) => get().session?.permissions.includes(p) ?? false,
}));
