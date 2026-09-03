// Bounces authenticated users away from /login (refresh, back-button).

import type { ReactNode } from "react";
import { Navigate } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";

export const AnonymousOnly = ({ children }: { children: ReactNode }) => {
  const { user, ready } = useAuth();
  if (!ready) return <div className="loading">Loading…</div>;
  if (user) return <Navigate to="/" replace />;
  return <>{children}</>;
};
