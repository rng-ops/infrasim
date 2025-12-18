import React from "react";
import { Routes, Route, NavLink, useNavigate, Navigate } from "react-router-dom";
import { useMemo } from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, SkipLink, StatusChip, PageHeader, DesignSystemStyles } from "@infrasim/ui";
import { ApiProvider } from "./api-context";
import { useSelector, useStore } from "./store/store";

// Meshnet Console MVP - Primary Experience
import MeshnetDashboard from "./pages/MeshnetDashboard";
import MeshnetLogin from "./pages/MeshnetLogin";

// Loading spinner for boot state
function BootingScreen() {
  return (
    <div className="app-booting" data-testid="booting-screen">
      <div className="booting-spinner" />
      <p>Loading Meshnet Console…</p>
    </div>
  );
}

// Main authenticated app shell - Meshnet focused
function AuthenticatedApp() {
  const navigate = useNavigate();
  const { actions } = useStore();

  const logout = () => {
    // Clear meshnet session
    fetch("/api/meshnet/auth/logout", { method: "POST", credentials: "include" })
      .finally(() => {
        actions.logout();
        navigate("/login");
      });
  };

  return (
    <div className="app-shell" data-testid="app-shell">
      <SkipLink />
      <header className="app-header" role="banner" data-testid="app-header">
        <div className="brand" onClick={() => navigate("/")} tabIndex={0} role="link" aria-label="Meshnet home">
          <div className="logo" aria-hidden>
            <span className="dot" />
          </div>
          <div>
            <div className="title">Meshnet Console</div>
            <div className="subtitle">Identity • Mesh • Appliances</div>
          </div>
        </div>
        <div className="header-actions" aria-label="Session actions">
          <Button variant="ghost" onClick={logout} aria-label="Log out" data-testid="logout-button">Log out</Button>
        </div>
      </header>
      <div className="app-body">
        <nav className="app-nav" aria-label="Primary" data-testid="app-nav">
          <NavItem to="/" label="Dashboard" />
        </nav>
        <main id="main" className="app-main" role="main" data-testid="app-main">
          <Routes>
            <Route path="/" element={<MeshnetDashboard />} />
            <Route path="*" element={<NotFound />} />
          </Routes>
        </main>
      </div>
    </div>
  );
}

export default function App() {
  const navigate = useNavigate();
  const { actions } = useStore();
  const authStatus = useSelector(s => s.auth.status);

  const handleMeshnetLogin = () => {
    // Meshnet uses its own session cookies, just update UI state
    actions.setAuthStatus("authenticated");
    navigate("/");
  };

  // While booting, check meshnet session
  if (authStatus === "booting") {
    // Check if we have a valid meshnet session
    fetch("/api/meshnet/me", { credentials: "include" })
      .then(res => {
        if (res.ok) {
          actions.setAuthStatus("authenticated");
        } else {
          actions.setAuthStatus("unauthenticated");
        }
      })
      .catch(() => {
        actions.setAuthStatus("unauthenticated");
      });

    return (
      <ApiProvider>
        <DesignSystemStyles />
        <BootingScreen />
      </ApiProvider>
    );
  }

  // Unauthenticated: show meshnet passkey login
  if (authStatus === "unauthenticated") {
    return (
      <ApiProvider>
        <DesignSystemStyles />
        <div className="app-shell app-shell--login" data-testid="login-shell">
          <Routes>
            <Route path="/login" element={<MeshnetLogin onLogin={handleMeshnetLogin} />} />
            <Route path="*" element={<Navigate to="/login" replace />} />
          </Routes>
        </div>
      </ApiProvider>
    );
  }

  // Authenticated: meshnet dashboard
  return (
    <ApiProvider>
      <DesignSystemStyles />
      <Routes>
        <Route path="/login" element={<Navigate to="/" replace />} />
        <Route path="/*" element={<AuthenticatedApp />} />
      </Routes>
    </ApiProvider>
  );
}

function NavItem({ to, label }: { to: string; label: string }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }: { isActive: boolean }) => (isActive ? "nav-link active" : "nav-link")}
    >
      {label}
    </NavLink>
  );
}

function NotFound() {
  return (
    <div>
      <PageHeader title="Not found" description="The page you are looking for does not exist." />
      <Card>
        <p>Try returning to the dashboard.</p>
      </Card>
    </div>
  );
}
