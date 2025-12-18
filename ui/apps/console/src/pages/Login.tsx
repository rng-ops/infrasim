import React, { useEffect, useState } from "react";
import { Button, Card, PageHeader } from "@infrasim/ui";

const TOKEN_KEY = "infrasim.token";

interface AuthStatus {
  needs_setup: boolean;
  identity_count: number;
  has_totp_enabled: boolean;
}

interface EnrollData {
  issuer: string;
  label: string;
  secret_b32: string;
  otpauth_uri: string;
  qr_svg: string;
}

type Step = "loading" | "setup" | "login";

export default function Login({ onLogin }: { onLogin: (token: string) => void }) {
  const [step, setStep] = useState<Step>("loading");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [enroll, setEnroll] = useState<EnrollData | null>(null);

  // The default identity for single-user mode
  const IDENTITY_NAME = "admin";

  // Check auth status on mount
  useEffect(() => {
    (async () => {
      try {
        const resp = await fetch("/api/auth/status");
        if (!resp.ok) {
          // Fallback to login if status endpoint fails
          setStep("login");
          return;
        }
        const status: AuthStatus = await resp.json();
        if (status.needs_setup || !status.has_totp_enabled) {
          // First-time setup: show QR immediately
          await beginSetup();
          setStep("setup");
        } else {
          setStep("login");
        }
      } catch {
        setStep("login");
      }
    })();
  }, []);

  const beginSetup = async () => {
    setBusy(true);
    setError(null);
    try {
      const resp = await fetch("/api/auth/totp/begin", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ display_name: IDENTITY_NAME }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(json?.error || `Setup failed (${resp.status})`);
      setEnroll(json);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  const confirmSetup = async () => {
    const c = code.trim();
    if (!c) {
      setError("Enter the 6-digit code from your authenticator");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const resp = await fetch("/api/auth/totp/confirm", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ display_name: IDENTITY_NAME, code: c }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(json?.error || `Confirm failed (${resp.status})`);
      
      // After confirming, auto-login
      await doLogin(c);
    } catch (e: any) {
      setError(e?.message || String(e));
      setBusy(false);
    }
  };

  const doLogin = async (codeOverride?: string) => {
    const c = (codeOverride || code).trim();
    if (!c) {
      setError("Enter the 6-digit code from your authenticator");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const resp = await fetch("/api/auth/totp/login", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ display_name: IDENTITY_NAME, code: c }),
      });
      const json = await resp.json().catch(() => ({}));
      if (!resp.ok) throw new Error(json?.error || `Login failed (${resp.status})`);
      const t = String(json?.token || "").trim();
      if (!t) throw new Error("Login did not return a token");
      sessionStorage.setItem(TOKEN_KEY, t);
      onLogin(t);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (step === "setup") {
      confirmSetup();
    } else {
      doLogin();
    }
  };

  // Loading state
  if (step === "loading") {
    return (
      <div className="login-page" data-testid="login-page">
        <div className="login-card">
          <div style={{ textAlign: "center", padding: "2rem" }}>
            <div className="spinner" style={{ 
              width: 40, height: 40, 
              border: "3px solid var(--ifm-color-border)",
              borderTopColor: "var(--ifm-color-accent)",
              borderRadius: "50%",
              animation: "spin 0.8s linear infinite",
              margin: "0 auto 1rem"
            }} />
            <p style={{ color: "var(--ifm-color-subtle)" }}>Checking setup status...</p>
          </div>
        </div>
      </div>
    );
  }

  // Setup flow - first time
  if (step === "setup") {
    return (
      <div className="login-page" data-testid="login-page">
        <div className="login-card">
          <h1>Welcome to InfraSim</h1>
          <p>Scan this QR code with Google Authenticator to set up access</p>

          {enroll ? (
            <div className="qr-section" data-testid="login-qr-section">
              <div
                dangerouslySetInnerHTML={{ __html: enroll.qr_svg }}
                style={{ maxWidth: 240, margin: "0 auto" }}
                data-testid="login-qr-code"
              />
              <div className="secret-code" data-testid="login-secret">
                {enroll.secret_b32}
              </div>
              <p style={{ fontSize: "0.875rem", color: "var(--ifm-color-subtle)", marginTop: "0.5rem" }}>
                Can't scan? Enter this code manually in your authenticator app
              </p>
            </div>
          ) : (
            <div style={{ textAlign: "center", padding: "1rem" }}>
              <Button onClick={beginSetup} disabled={busy}>
                {busy ? "Loading..." : "Generate QR Code"}
              </Button>
            </div>
          )}

          {enroll && (
            <form onSubmit={handleSubmit}>
              <label htmlFor="code">Enter the 6-digit code to verify</label>
              <input
                id="code"
                data-testid="login-code-input"
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                maxLength={6}
                autoComplete="one-time-code"
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
                placeholder="000000"
                autoFocus
              />

              {error && (
                <div className="login-error" data-testid="login-error">
                  {error}
                </div>
              )}

              <Button type="submit" disabled={busy || code.length !== 6} data-testid="login-submit-button">
                {busy ? "Verifying..." : "Complete Setup"}
              </Button>
            </form>
          )}
        </div>
      </div>
    );
  }

  // Normal login flow
  return (
    <div className="login-page" data-testid="login-page">
      <div className="login-card">
        <h1>InfraSim Login</h1>
        <p>Enter your authenticator code</p>

        <form onSubmit={handleSubmit}>
          <label htmlFor="code">6-digit code from Google Authenticator</label>
          <input
            id="code"
            data-testid="login-code-input"
            type="text"
            inputMode="numeric"
            pattern="[0-9]*"
            maxLength={6}
            autoComplete="one-time-code"
            value={code}
            onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
            placeholder="000000"
            autoFocus
          />

          {error && (
            <div className="login-error" data-testid="login-error">
              {error}
            </div>
          )}

          <Button type="submit" disabled={busy || code.length !== 6} data-testid="login-submit-button">
            {busy ? "Logging in..." : "Login"}
          </Button>
        </form>

        <div style={{ marginTop: "1.5rem", textAlign: "center" }}>
          <button
            type="button"
            onClick={() => { setStep("setup"); beginSetup(); }}
            style={{
              background: "none",
              border: "none",
              color: "var(--ifm-color-subtle)",
              fontSize: "0.875rem",
              cursor: "pointer",
              textDecoration: "underline",
            }}
          >
            Need to set up a new device?
          </button>
        </div>
      </div>
    </div>
  );
}
