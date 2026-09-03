// Login + registration. The seeded demo accounts sit one click away so
// reviewers are inside the app in seconds.

import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuth } from "../auth/AuthContext";

type Mode = "login" | "register";

const DEMO_ACCOUNTS = [
  { label: "Studio (ACME Bros)", email: "grace@acme.example", password: "nw-derulo-99", role: "STUDIO", org: "ACME Bros Pictures" },
  { label: "Label (Warp Records)", email: "warp-label@acme.example", password: "label-pass-99", role: "LABEL", org: "Warp Records" },
  { label: "Admin", email: "admin@acme.example", password: "admin-pass-99", role: "ADMIN" },
];

export const LoginPage = () => {
  const { login, register } = useAuth();
  const navigate = useNavigate();
  const [mode, setMode] = useState<Mode>("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [role, setRole] = useState("STUDIO");
  const [orgName, setOrgName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (mode === "login") {
        await login(email, password);
      } else {
        await register({
          email,
          password,
          display_name: displayName || email.split("@")[0],
          role,
          org_name: role === "ADMIN" ? undefined : orgName || `${displayName || email}'s org`,
        });
      }
      // The role-aware home route takes over from here.
      navigate("/", { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "something went wrong");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login-page">
      <div className="login-card">
        <h1>ACME Licensing</h1>
        <p className="subtitle">Music licensing for the pictures that need it.</p>

        <div className="mode-switch">
          <button type="button" className={mode === "login" ? "active" : ""} onClick={() => setMode("login")}>
            Sign in
          </button>
          <button type="button" className={mode === "register" ? "active" : ""} onClick={() => setMode("register")}>
            Register
          </button>
        </div>

        <form onSubmit={submit}>
          <label>
            Email
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              autoComplete="username"
            />
          </label>
          <label>
            Password
            <input
              type="password"
              required
              minLength={8}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete={mode === "login" ? "current-password" : "new-password"}
            />
          </label>

          {mode === "register" && (
            <>
              <label>
                Display name
                <input
                  required
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                />
              </label>
              <label>
                Role
                <select value={role} onChange={(e) => setRole(e.target.value)}>
                  <option value="STUDIO">Movie studio</option>
                  <option value="LABEL">Record label</option>
                  <option value="ADMIN">Administrator</option>
                </select>
              </label>
              {role !== "ADMIN" && (
                <label>
                  Organization
                  <input
                    required
                    value={orgName}
                    onChange={(e) => setOrgName(e.target.value)}
                    placeholder={role === "STUDIO" ? "ACME Bros Pictures" : "Warp Records"}
                  />
                </label>
              )}
            </>
          )}

          {error && <p className="error" role="alert">{error}</p>}

          <button type="submit" className="primary" disabled={busy}>
            {busy ? "…" : mode === "login" ? "Sign in" : "Create account"}
          </button>
        </form>

        <div className="demo-accounts">
          <p>Demo accounts (from <code>just seed</code>):</p>
          {DEMO_ACCOUNTS.map((account) => (
            <button
              key={account.email}
              type="button"
              className="demo"
              onClick={() => {
                setMode("login");
                setEmail(account.email);
                setPassword(account.password);
              }}
            >
              {account.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
