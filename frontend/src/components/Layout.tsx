// App layout: nav with role-aware links, user chip, notification bell.

import { Link, NavLink, Outlet, useNavigate } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";
import type { Role } from "../api/types";
import { NotificationBell } from "./NotificationBell";
import { useLicenseStream } from "../hooks/useLiveStreams";

const linksFor = (role: Role | undefined): { to: string; label: string }[] => {
  switch (role) {
    case "STUDIO":
      return [
        { to: "/movies", label: "Movies" },
        { to: "/search", label: "Find music" },
      ];
    case "LABEL":
      return [
        { to: "/catalog", label: "Catalog" },
        { to: "/licenses", label: "Incoming licenses" },
      ];
    default:
      return [];
  }
};

export const Layout = () => {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const { connected: licensesLive } = useLicenseStream();

  return (
    <div className="app">
      <header>
        <Link to="/" className="brand">
          ACME <span>Licensing</span>
        </Link>
        <nav>
          {linksFor(user?.role).map((link) => (
            <NavLink key={link.to} to={link.to}>
              {link.label}
            </NavLink>
          ))}
        </nav>
        <div className="header-right">
          {user && user.role !== "ADMIN" && (
            <span className={`badge live ${licensesLive ? "" : "off"}`}
              style={licensesLive ? undefined : { opacity: 0.3 }}
              title={licensesLive ? "Live license updates connected" : "Connecting…"}>
              {licensesLive ? "live" : "…"}
            </span>
          )}
          {user && user.role !== "ADMIN" && <NotificationBell />}
          {user && (
            <span className="user-chip" title={user.email}>
              <strong>{user.display_name}</strong>
              <em>{user.role}</em>
            </span>
          )}
          <button
            type="button"
            className="ghost"
            onClick={async () => {
              await logout();
              navigate("/login");
            }}
          >
            Sign out
          </button>
        </div>
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  );
};
