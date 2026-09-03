// Authentication context: in-memory access token, silent refresh on boot,
// login/logout (ADR-012).

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { refresh, setAccessToken } from "../api/client";
import { auth } from "../api/endpoints";
import type { User } from "../api/types";

interface AuthState {
  user: User | null;
  ready: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (input: {
    email: string;
    password: string;
    display_name: string;
    role: string;
    org_name?: string;
  }) => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthState | null>(null);

export const AuthProvider = ({ children }: { children: ReactNode }) => {
  const [user, setUser] = useState<User | null>(null);
  const [ready, setReady] = useState(false);

  // Silent refresh on boot: the HttpOnly cookie restores the session.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (await refresh()) {
        try {
          const me = await auth.me();
          if (!cancelled) setUser(me);
        } catch {
          setAccessToken(null);
        }
      }
      if (!cancelled) setReady(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const body = await auth.login({ email, password });
    setAccessToken(body.access_token);
    setUser(body.user);
  }, []);

  const register = useCallback<AuthState["register"]>(async (input) => {
    const body = await auth.register(input);
    setAccessToken(body.access_token);
    setUser(body.user);
  }, []);

  const logout = useCallback(async () => {
    try {
      await auth.logout(); // revokes the refresh family server-side
    } finally {
      setAccessToken(null);
      setUser(null);
    }
  }, []);

  const value = useMemo(
    () => ({ user, ready, login, register, logout }),
    [user, ready, login, register, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
};

export const useAuth = (): AuthState => {
  const context = useContext(AuthContext);
  if (!context) throw new Error("useAuth outside AuthProvider");
  return context;
};
