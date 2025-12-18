import React, { createContext, useContext, useEffect, useMemo } from "react";
import { createApiClient } from "@infrasim/api-client";
import { useStore, useSelector } from "./store/store";

// ============================================================================
// API Context
// ============================================================================

type ApiClient = ReturnType<typeof createApiClient>;

const ApiContext = createContext<ApiClient | null>(null);

export function useApi(): ApiClient {
  const ctx = useContext(ApiContext);
  if (!ctx) throw new Error("useApi must be used within ApiProvider");
  return ctx;
}

// ============================================================================
// Auth Bootstrap Hook - verifies stored token on app boot
// ============================================================================

function useAuthBootstrap() {
  const { state, actions } = useStore();
  const { status, token } = state.auth;

  useEffect(() => {
    if (status !== "booting") return;
    
    // No token stored - go directly to unauthenticated
    if (!token) {
      actions.setAuthStatus("unauthenticated");
      return;
    }

    // Verify the stored token
    const verify = async () => {
      try {
        const res = await fetch("/api/auth/whoami", {
          headers: { Authorization: `Bearer ${token}` },
        });
        if (res.ok) {
          const data = await res.json();
          actions.setIdentity({ display_name: data.display_name, created_at: data.created_at });
          actions.setAuthStatus("authenticated");
        } else {
          // Token invalid - clear and go to login
          actions.logout();
        }
      } catch {
        // Network error during verification - go to unauthenticated
        actions.logout();
      }
    };

    verify();
  }, [status, token, actions]);
}

// ============================================================================
// API Provider
// ============================================================================

export function ApiProvider({ children }: { children: React.ReactNode }) {
  const { state, actions } = useStore();

  // Bootstrap auth on mount
  useAuthBootstrap();

  const api = useMemo(() => {
    return createApiClient({
      baseUrl: "", // same origin
      getToken: () => state.auth.token,
      onUnauthorized: () => {
        actions.logout();
      },
    });
  }, [state.auth.token, actions]);

  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>;
}

export { ApiContext };
