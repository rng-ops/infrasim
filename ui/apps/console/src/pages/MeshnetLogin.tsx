/**
 * Meshnet Console MVP - WebAuthn Passkey Login
 * 
 * Handles both registration and login flows using WebAuthn passkeys.
 * No passwords - passkeys only.
 */
import React, { useState, useCallback } from "react";
import { Button, Card } from "@infrasim/ui";
import {
  startRegistration,
  startAuthentication,
  browserSupportsWebAuthn,
} from "@simplewebauthn/browser";

// =============================================================================
// Types
// =============================================================================

interface RegisterOptionsResponse {
  challenge_id: string;
  options: PublicKeyCredentialCreationOptionsJSON;
}

interface LoginOptionsResponse {
  challenge_id: string;
  options: PublicKeyCredentialRequestOptionsJSON;
}

interface AuthResponse {
  token: string;
  expires_at: number;
  user: {
    id: string;
    created_at: number;
  };
}

interface MeshnetLoginProps {
  onLogin: (token: string) => void;
}

const API_BASE = "/api/meshnet";

// =============================================================================
// Main Component
// =============================================================================

export default function MeshnetLogin({ onLogin }: MeshnetLoginProps) {
  const [mode, setMode] = useState<"idle" | "register" | "login">("idle");
  const [handle, setHandle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const supportsWebAuthn = browserSupportsWebAuthn();

  // Registration flow
  const handleRegister = useCallback(async () => {
    if (!handle.trim()) {
      setError("Please enter a handle");
      return;
    }

    setBusy(true);
    setError(null);

    try {
      // 1. Get registration options from server
      const optionsRes = await fetch(`${API_BASE}/auth/register/options`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ handle: handle.trim() }),
      });
      
      if (!optionsRes.ok) {
        const err = await optionsRes.json().catch(() => ({}));
        throw new Error(err.error || "Failed to get registration options");
      }
      
      const optionsData: RegisterOptionsResponse = await optionsRes.json();
      
      // 2. Prompt user to create passkey
      const credential = await startRegistration(optionsData.options);
      
      // 3. Verify with server
      const verifyRes = await fetch(`${API_BASE}/auth/register/verify`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          challenge_id: optionsData.challenge_id,
          handle: handle.trim(),
          credential,
        }),
      });
      
      if (!verifyRes.ok) {
        const err = await verifyRes.json().catch(() => ({}));
        throw new Error(err.error || "Registration verification failed");
      }
      
      const authData: AuthResponse = await verifyRes.json();
      
      // 4. Store token and complete login
      sessionStorage.setItem("meshnet.token", authData.token);
      onLogin(authData.token);
      
    } catch (err: any) {
      if (err.name === "NotAllowedError") {
        setError("Passkey creation was cancelled or timed out");
      } else {
        setError(err.message || "Registration failed");
      }
    } finally {
      setBusy(false);
    }
  }, [handle, onLogin]);

  // Login flow
  const handleLogin = useCallback(async () => {
    if (!handle.trim()) {
      setError("Please enter your handle");
      return;
    }

    setBusy(true);
    setError(null);

    try {
      // 1. Get authentication options from server
      const optionsRes = await fetch(`${API_BASE}/auth/login/options`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ handle: handle.trim() }),
      });
      
      if (!optionsRes.ok) {
        const err = await optionsRes.json().catch(() => ({}));
        throw new Error(err.error || "Failed to get login options");
      }
      
      const optionsData: LoginOptionsResponse = await optionsRes.json();
      
      // 2. Prompt user to authenticate with passkey
      const credential = await startAuthentication(optionsData.options);
      
      // 3. Verify with server
      const verifyRes = await fetch(`${API_BASE}/auth/login/verify`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          challenge_id: optionsData.challenge_id,
          credential,
        }),
      });
      
      if (!verifyRes.ok) {
        const err = await verifyRes.json().catch(() => ({}));
        throw new Error(err.error || "Authentication failed");
      }
      
      const authData: AuthResponse = await verifyRes.json();
      
      // 4. Store token and complete login
      sessionStorage.setItem("meshnet.token", authData.token);
      onLogin(authData.token);
      
    } catch (err: any) {
      if (err.name === "NotAllowedError") {
        setError("Authentication was cancelled or timed out");
      } else {
        setError(err.message || "Login failed");
      }
    } finally {
      setBusy(false);
    }
  }, [handle, onLogin]);

  // Check WebAuthn support
  if (!supportsWebAuthn) {
    return (
      <div className="meshnet-login">
        <div className="login-card">
          <h1>Meshnet Console</h1>
          <p style={{ color: "var(--ifm-color-danger)" }}>
            Your browser doesn't support WebAuthn passkeys.
            Please use a modern browser like Chrome, Firefox, Safari, or Edge.
          </p>
        </div>
        <style>{loginStyles}</style>
      </div>
    );
  }

  return (
    <div className="meshnet-login">
      <div className="login-card">
        <div className="login-header">
          <div className="logo">
            <span className="logo-dot" />
          </div>
          <h1>Meshnet Console</h1>
          <p>Secure mesh networking with passkeys</p>
        </div>

        {mode === "idle" && (
          <div className="login-choices">
            <p>Welcome! Do you have an existing passkey?</p>
            <div className="button-group">
              <Button onClick={() => setMode("login")}>
                Sign In
              </Button>
              <Button variant="secondary" onClick={() => setMode("register")}>
                Create Account
              </Button>
            </div>
          </div>
        )}

        {mode === "register" && (
          <form onSubmit={e => { e.preventDefault(); handleRegister(); }}>
            <h2>Create your identity</h2>
            <p className="form-hint">
              Choose a unique handle. This will be your subdomain and Matrix ID.
            </p>
            
            <label htmlFor="handle">Handle</label>
            <input
              id="handle"
              type="text"
              value={handle}
              onChange={e => setHandle(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))}
              placeholder="myhandle"
              maxLength={32}
              autoFocus
              autoComplete="username"
            />
            <p className="input-hint">
              {handle && `${handle}.mesh.local • @${handle}:matrix.mesh.local`}
            </p>

            {error && <div className="error-message">{error}</div>}

            <Button type="submit" disabled={busy || handle.length < 3}>
              {busy ? "Creating passkey..." : "Create with Passkey"}
            </Button>
            
            <button type="button" className="link-button" onClick={() => { setMode("idle"); setError(null); }}>
              ← Back
            </button>
          </form>
        )}

        {mode === "login" && (
          <form onSubmit={e => { e.preventDefault(); handleLogin(); }}>
            <h2>Sign in</h2>
            <p className="form-hint">
              Enter your handle to authenticate with your passkey.
            </p>
            
            <label htmlFor="handle">Handle</label>
            <input
              id="handle"
              type="text"
              value={handle}
              onChange={e => setHandle(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))}
              placeholder="myhandle"
              maxLength={32}
              autoFocus
              autoComplete="username webauthn"
            />

            {error && <div className="error-message">{error}</div>}

            <Button type="submit" disabled={busy || handle.length < 3}>
              {busy ? "Authenticating..." : "Sign in with Passkey"}
            </Button>
            
            <button type="button" className="link-button" onClick={() => { setMode("idle"); setError(null); }}>
              ← Back
            </button>
          </form>
        )}

        <div className="passkey-info">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 12.5a5.5 5.5 0 110-11 5.5 5.5 0 010 11z"/>
            <path d="M8 4.5a1 1 0 00-.75.34l-2.5 2.82a.75.75 0 001.12 1l1.38-1.56v4.65a.75.75 0 001.5 0V7.1l1.38 1.56a.75.75 0 001.12-1L8.75 4.84A1 1 0 008 4.5z"/>
          </svg>
          <span>Passkeys are secure, phishing-resistant credentials stored on your device.</span>
        </div>
      </div>
      
      <style>{loginStyles}</style>
    </div>
  );
}

const loginStyles = `
  .meshnet-login {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #0f172a 0%, #1e293b 100%);
    padding: 1rem;
  }
  
  .login-card {
    background: var(--ifm-color-surface, #1e293b);
    border: 1px solid var(--ifm-color-border, #334155);
    border-radius: 12px;
    padding: 2rem;
    width: 100%;
    max-width: 400px;
    box-shadow: 0 4px 24px rgba(0,0,0,0.3);
  }
  
  .login-header {
    text-align: center;
    margin-bottom: 2rem;
  }
  
  .login-header h1 {
    font-size: 1.5rem;
    font-weight: 600;
    margin: 0.5rem 0 0.25rem;
    color: var(--ifm-color-text, #f8fafc);
  }
  
  .login-header p {
    font-size: 0.875rem;
    color: var(--ifm-color-subtle, #94a3b8);
    margin: 0;
  }
  
  .logo {
    width: 48px;
    height: 48px;
    margin: 0 auto;
    background: linear-gradient(135deg, #3b82f6, #8b5cf6);
    border-radius: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  
  .logo-dot {
    width: 16px;
    height: 16px;
    background: white;
    border-radius: 50%;
    box-shadow: 0 0 12px rgba(255,255,255,0.5);
  }
  
  .login-choices {
    text-align: center;
  }
  
  .login-choices p {
    margin-bottom: 1.5rem;
    color: var(--ifm-color-subtle, #94a3b8);
  }
  
  .button-group {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .login-card form h2 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0 0 0.5rem;
    color: var(--ifm-color-text, #f8fafc);
  }
  
  .form-hint {
    font-size: 0.875rem;
    color: var(--ifm-color-subtle, #94a3b8);
    margin-bottom: 1.5rem;
  }
  
  .login-card label {
    display: block;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--ifm-color-text, #f8fafc);
    margin-bottom: 0.5rem;
  }
  
  .login-card input {
    width: 100%;
    padding: 0.75rem;
    border: 1px solid var(--ifm-color-border, #334155);
    border-radius: 6px;
    background: var(--ifm-color-surface-muted, #0f172a);
    color: var(--ifm-color-text, #f8fafc);
    font-size: 1rem;
    margin-bottom: 0.5rem;
    transition: border-color 0.15s;
  }
  
  .login-card input:focus {
    outline: none;
    border-color: var(--ifm-color-accent, #3b82f6);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.25);
  }
  
  .input-hint {
    font-size: 0.75rem;
    color: var(--ifm-color-subtle, #64748b);
    margin-bottom: 1rem;
    font-family: monospace;
    min-height: 1rem;
  }
  
  .error-message {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #f87171;
    padding: 0.75rem;
    border-radius: 6px;
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }
  
  .login-card button[type="submit"] {
    width: 100%;
    margin-top: 0.5rem;
  }
  
  .link-button {
    display: block;
    width: 100%;
    margin-top: 1rem;
    padding: 0.5rem;
    background: none;
    border: none;
    color: var(--ifm-color-subtle, #94a3b8);
    font-size: 0.875rem;
    cursor: pointer;
    transition: color 0.15s;
  }
  
  .link-button:hover {
    color: var(--ifm-color-text, #f8fafc);
  }
  
  .passkey-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 1px solid var(--ifm-color-border, #334155);
    font-size: 0.75rem;
    color: var(--ifm-color-subtle, #64748b);
  }
  
  .passkey-info svg {
    flex-shrink: 0;
    opacity: 0.5;
  }
`;
