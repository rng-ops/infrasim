import React, { useContext, useEffect, useMemo, useReducer } from "react";

// ============================================================================
// Comprehensive Vuex-like store for InfraSim Console
// ============================================================================
// - State is immutable
// - Mutations are the only way to change state
// - Actions orchestrate async work and commit mutations
// - Selectors compute derived data
// - Designed for: fleet view, graph editing, snapshots, restore, trust/attestation
// ============================================================================

// ============================================================================
// State Types
// ============================================================================

// Auth status: Booting = checking session, Unauthenticated = no valid session, Authenticated = valid session
export type AuthStatus = "booting" | "unauthenticated" | "authenticated";

export type AuthState = {
  status: AuthStatus;
  token: string | null;
  adminToken: string | null;
  viewToken: string | null;
  capabilities: string[];
  capabilitiesExpiry: number | null;
  identity: { display_name: string; created_at: string } | null;
};

export type ToastItem = {
  id: string;
  title: string;
  description?: string;
  tone?: "info" | "success" | "danger";
};

export type UiState = {
  toasts: ToastItem[];
  isPageVisible: boolean;
  sidebarCollapsed: boolean;
  activeModal: string | null;
  confirmDialog: ConfirmDialogState | null;
};

export type ConfirmDialogState = {
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel: string;
  tone: "danger" | "warning" | "info";
  onConfirm: () => void;
  onCancel?: () => void;
};

export type ApplianceRecord = {
  id: string;
  name: string;
  templateId: string;
  status: string;
  vmId: string | null;
  networkIds: string[];
  volumeIds: string[];
  consoleId: string | null;
  snapshotIds: string[];
  createdAt: number;
  updatedAt: number;
};

export type AppliancesState = {
  byId: Record<string, ApplianceRecord>;
  allIds: string[];
  filters: { status: string[]; templateId: string[]; search: string };
  selection: string[];
  bulkBusy: boolean;
  lastFetchedAt: number | null;
};

export type GraphNode = {
  id: string;
  type: "appliance" | "network" | "volume" | "service";
  label: string;
  position: { x: number; y: number };
  data: Record<string, unknown>;
};

export type GraphEdge = {
  id: string;
  source: string;
  target: string;
  edgeType: string;
  data: Record<string, unknown>;
};

export type GraphState = {
  current: { nodes: GraphNode[]; edges: GraphEdge[]; version: string } | null;
  draft: { nodes: GraphNode[]; edges: GraphEdge[] } | null;
  selection: { nodes: string[]; edges: string[] };
  validation: { valid: boolean; errors: string[] };
  applyBusy: boolean;
  planResult: PlanResult | null;
};

export type PlanResult = {
  adds: { type: string; id: string; name: string }[];
  updates: { type: string; id: string; changes: string[] }[];
  deletes: { type: string; id: string; name: string }[];
  warnings: string[];
};

export type SnapshotRecord = {
  id: string;
  name: string;
  vmId: string;
  applianceId?: string;
  includeMemory: boolean;
  includeDisk: boolean;
  description: string;
  complete: boolean;
  digest: string;
  sizeBytes: number;
  encrypted: boolean;
  createdAt: number;
};

export type SnapshotsState = {
  byId: Record<string, SnapshotRecord>;
  allIds: string[];
  createBusy: boolean;
  lastResult: { success: boolean; snapshotId?: string; error?: string } | null;
};

export type RestoreStep = {
  id: string;
  label: string;
  status: "pending" | "running" | "completed" | "error";
  message?: string;
};

export type RestoreState = {
  currentCommit: string | null;
  steps: RestoreStep[];
  progress: number;
  errors: string[];
  verification: {
    signatureValid: boolean | null;
    policyValid: boolean | null;
    attestationValid: boolean | null;
    verdict: "trusted" | "untrusted" | "unknown";
  };
};

export type TrustLevel = "local" | "remote" | "attested";

export type TrustRecord = {
  applianceId: string;
  level: TrustLevel;
  attestationType?: string;
  digest?: string;
  signature?: string;
  verifiedAt?: number;
};

export type TrustState = {
  byAppliance: Record<string, TrustRecord>;
  bySurface: Record<string, TrustRecord>;
  keysRegistry: Record<string, { publicKey: string; issuer: string; expiresAt: number }>;
  policy: { requireAttested: boolean; allowRemote: boolean; minimumTrustLevel: TrustLevel };
};

export type AuditEvent = {
  id: string;
  type: string;
  timestamp: number;
  applianceId?: string;
  userId?: string;
  action: string;
  outcome: "success" | "failure";
  details: Record<string, unknown>;
  signature?: string;
};

export type EventsState = {
  buffer: AuditEvent[];
  bufferMaxSize: number;
  index: { byType: Record<string, string[]>; byAppliance: Record<string, string[]> };
  sseConnected: boolean;
  lastEventId: string | null;
};

export type RenderChannelState = {
  channels: Record<string, { lastPatchAtMs: number; patch: Record<string, unknown> }>;
};

export type TemplateRecord = {
  id: string;
  title: string;
  description: string;
  arch: string;
  machine: string;
  cpuCores: number;
  memoryMb: number;
  tags: string[];
  image?: string;
};

export type TemplatesState = {
  byId: Record<string, TemplateRecord>;
  allIds: string[];
  lastFetchedAt: number | null;
};

export type AppState = {
  auth: AuthState;
  ui: UiState;
  appliances: AppliancesState;
  templates: TemplatesState;
  graph: GraphState;
  snapshots: SnapshotsState;
  restore: RestoreState;
  trust: TrustState;
  events: EventsState;
  render: RenderChannelState;
};

// ============================================================================
// Mutations
// ============================================================================

export type Mutation =
  | { type: "auth/setStatus"; status: AuthStatus }
  | { type: "auth/setToken"; token: string | null }
  | { type: "auth/setAdminToken"; token: string | null }
  | { type: "auth/setViewToken"; token: string | null }
  | { type: "auth/setCapabilities"; capabilities: string[]; expiry: number }
  | { type: "auth/setIdentity"; identity: AuthState["identity"] }
  | { type: "ui/pushToast"; toast: ToastItem }
  | { type: "ui/dismissToast"; id: string }
  | { type: "ui/setVisibility"; isVisible: boolean }
  | { type: "ui/toggleSidebar" }
  | { type: "ui/setModal"; modalId: string | null }
  | { type: "ui/showConfirm"; dialog: ConfirmDialogState }
  | { type: "ui/hideConfirm" }
  | { type: "appliances/setAll"; appliances: ApplianceRecord[] }
  | { type: "appliances/upsert"; appliance: ApplianceRecord }
  | { type: "appliances/remove"; id: string }
  | { type: "appliances/setFilters"; filters: Partial<AppliancesState["filters"]> }
  | { type: "appliances/setSelection"; ids: string[] }
  | { type: "appliances/toggleSelection"; id: string }
  | { type: "appliances/setBulkBusy"; busy: boolean }
  | { type: "templates/setAll"; templates: TemplateRecord[] }
  | { type: "graph/setCurrent"; graph: GraphState["current"] }
  | { type: "graph/setDraft"; draft: GraphState["draft"] }
  | { type: "graph/updateDraftNode"; nodeId: string; updates: Partial<GraphNode> }
  | { type: "graph/addDraftEdge"; edge: GraphEdge }
  | { type: "graph/removeDraftEdge"; edgeId: string }
  | { type: "graph/setSelection"; selection: GraphState["selection"] }
  | { type: "graph/setValidation"; validation: GraphState["validation"] }
  | { type: "graph/setApplyBusy"; busy: boolean }
  | { type: "graph/setPlanResult"; plan: PlanResult | null }
  | { type: "snapshots/setAll"; snapshots: SnapshotRecord[] }
  | { type: "snapshots/upsert"; snapshot: SnapshotRecord }
  | { type: "snapshots/setCreateBusy"; busy: boolean }
  | { type: "snapshots/setLastResult"; result: SnapshotsState["lastResult"] }
  | { type: "restore/start"; commit: string; steps: RestoreStep[] }
  | { type: "restore/updateStep"; stepId: string; status: RestoreStep["status"]; message?: string }
  | { type: "restore/setProgress"; progress: number }
  | { type: "restore/addError"; error: string }
  | { type: "restore/setVerification"; verification: Partial<RestoreState["verification"]> }
  | { type: "restore/reset" }
  | { type: "trust/setAppliance"; record: TrustRecord }
  | { type: "trust/setSurface"; surfaceId: string; record: TrustRecord }
  | { type: "trust/setPolicy"; policy: Partial<TrustState["policy"]> }
  | { type: "trust/addKey"; keyId: string; key: TrustState["keysRegistry"][string] }
  | { type: "events/push"; event: AuditEvent }
  | { type: "events/setConnected"; connected: boolean }
  | { type: "events/setLastEventId"; id: string }
  | { type: "render/patch"; channelId: string; patch: Record<string, unknown> };

// ============================================================================
// Initial State
// ============================================================================

export function createInitialState(): AppState {
  const storedToken = sessionStorage.getItem("infrasim.token");
  return {
    auth: {
      status: storedToken ? "booting" : "unauthenticated", // If we have a stored token, boot to verify it
      token: storedToken,
      adminToken: sessionStorage.getItem("infrasim.adminToken"),
      viewToken: null,
      capabilities: [],
      capabilitiesExpiry: null,
      identity: null,
    },
    ui: {
      toasts: [],
      isPageVisible: typeof document === "undefined" ? true : document.visibilityState !== "hidden",
      sidebarCollapsed: false,
      activeModal: null,
      confirmDialog: null,
    },
    appliances: {
      byId: {},
      allIds: [],
      filters: { status: [], templateId: [], search: "" },
      selection: [],
      bulkBusy: false,
      lastFetchedAt: null,
    },
    templates: { byId: {}, allIds: [], lastFetchedAt: null },
    graph: {
      current: null,
      draft: null,
      selection: { nodes: [], edges: [] },
      validation: { valid: true, errors: [] },
      applyBusy: false,
      planResult: null,
    },
    snapshots: { byId: {}, allIds: [], createBusy: false, lastResult: null },
    restore: {
      currentCommit: null,
      steps: [],
      progress: 0,
      errors: [],
      verification: { signatureValid: null, policyValid: null, attestationValid: null, verdict: "unknown" },
    },
    trust: {
      byAppliance: {},
      bySurface: {},
      keysRegistry: {},
      policy: { requireAttested: false, allowRemote: true, minimumTrustLevel: "local" },
    },
    events: {
      buffer: [],
      bufferMaxSize: 500,
      index: { byType: {}, byAppliance: {} },
      sseConnected: false,
      lastEventId: null,
    },
    render: { channels: {} },
  };
}

// ============================================================================
// Reducer
// ============================================================================

function reducer(state: AppState, m: Mutation): AppState {
  switch (m.type) {
    case "auth/setStatus":
      return { ...state, auth: { ...state.auth, status: m.status } };
    case "auth/setToken": {
      if (m.token) sessionStorage.setItem("infrasim.token", m.token);
      else sessionStorage.removeItem("infrasim.token");
      return { ...state, auth: { ...state.auth, token: m.token } };
    }
    case "auth/setAdminToken": {
      if (m.token) sessionStorage.setItem("infrasim.adminToken", m.token);
      else sessionStorage.removeItem("infrasim.adminToken");
      return { ...state, auth: { ...state.auth, adminToken: m.token } };
    }
    case "auth/setViewToken":
      return { ...state, auth: { ...state.auth, viewToken: m.token } };
    case "auth/setCapabilities":
      return { ...state, auth: { ...state.auth, capabilities: m.capabilities, capabilitiesExpiry: m.expiry } };
    case "auth/setIdentity":
      return { ...state, auth: { ...state.auth, identity: m.identity } };
    case "ui/pushToast": {
      const next = [m.toast, ...state.ui.toasts].slice(0, 4);
      return { ...state, ui: { ...state.ui, toasts: next } };
    }
    case "ui/dismissToast":
      return { ...state, ui: { ...state.ui, toasts: state.ui.toasts.filter((t) => t.id !== m.id) } };
    case "ui/setVisibility":
      return { ...state, ui: { ...state.ui, isPageVisible: m.isVisible } };
    case "ui/toggleSidebar":
      return { ...state, ui: { ...state.ui, sidebarCollapsed: !state.ui.sidebarCollapsed } };
    case "ui/setModal":
      return { ...state, ui: { ...state.ui, activeModal: m.modalId } };
    case "ui/showConfirm":
      return { ...state, ui: { ...state.ui, confirmDialog: m.dialog } };
    case "ui/hideConfirm":
      return { ...state, ui: { ...state.ui, confirmDialog: null } };
    case "appliances/setAll": {
      const byId: Record<string, ApplianceRecord> = {};
      const allIds: string[] = [];
      for (const a of m.appliances) { byId[a.id] = a; allIds.push(a.id); }
      return { ...state, appliances: { ...state.appliances, byId, allIds, lastFetchedAt: Date.now() } };
    }
    case "appliances/upsert": {
      const byId = { ...state.appliances.byId, [m.appliance.id]: m.appliance };
      const allIds = state.appliances.allIds.includes(m.appliance.id) ? state.appliances.allIds : [...state.appliances.allIds, m.appliance.id];
      return { ...state, appliances: { ...state.appliances, byId, allIds } };
    }
    case "appliances/remove": {
      const { [m.id]: _, ...byId } = state.appliances.byId;
      const allIds = state.appliances.allIds.filter((id) => id !== m.id);
      const selection = state.appliances.selection.filter((id) => id !== m.id);
      return { ...state, appliances: { ...state.appliances, byId, allIds, selection } };
    }
    case "appliances/setFilters":
      return { ...state, appliances: { ...state.appliances, filters: { ...state.appliances.filters, ...m.filters } } };
    case "appliances/setSelection":
      return { ...state, appliances: { ...state.appliances, selection: m.ids } };
    case "appliances/toggleSelection": {
      const selection = state.appliances.selection.includes(m.id)
        ? state.appliances.selection.filter((id) => id !== m.id)
        : [...state.appliances.selection, m.id];
      return { ...state, appliances: { ...state.appliances, selection } };
    }
    case "appliances/setBulkBusy":
      return { ...state, appliances: { ...state.appliances, bulkBusy: m.busy } };
    case "templates/setAll": {
      const byId: Record<string, TemplateRecord> = {};
      const allIds: string[] = [];
      for (const t of m.templates) { byId[t.id] = t; allIds.push(t.id); }
      return { ...state, templates: { ...state.templates, byId, allIds, lastFetchedAt: Date.now() } };
    }
    case "graph/setCurrent":
      return { ...state, graph: { ...state.graph, current: m.graph } };
    case "graph/setDraft":
      return { ...state, graph: { ...state.graph, draft: m.draft } };
    case "graph/updateDraftNode": {
      if (!state.graph.draft) return state;
      const nodes = state.graph.draft.nodes.map((n) => (n.id === m.nodeId ? { ...n, ...m.updates } : n));
      return { ...state, graph: { ...state.graph, draft: { ...state.graph.draft, nodes } } };
    }
    case "graph/addDraftEdge": {
      if (!state.graph.draft) return state;
      return { ...state, graph: { ...state.graph, draft: { ...state.graph.draft, edges: [...state.graph.draft.edges, m.edge] } } };
    }
    case "graph/removeDraftEdge": {
      if (!state.graph.draft) return state;
      const edges = state.graph.draft.edges.filter((e) => e.id !== m.edgeId);
      return { ...state, graph: { ...state.graph, draft: { ...state.graph.draft, edges } } };
    }
    case "graph/setSelection":
      return { ...state, graph: { ...state.graph, selection: m.selection } };
    case "graph/setValidation":
      return { ...state, graph: { ...state.graph, validation: m.validation } };
    case "graph/setApplyBusy":
      return { ...state, graph: { ...state.graph, applyBusy: m.busy } };
    case "graph/setPlanResult":
      return { ...state, graph: { ...state.graph, planResult: m.plan } };
    case "snapshots/setAll": {
      const byId: Record<string, SnapshotRecord> = {};
      const allIds: string[] = [];
      for (const s of m.snapshots) { byId[s.id] = s; allIds.push(s.id); }
      return { ...state, snapshots: { ...state.snapshots, byId, allIds } };
    }
    case "snapshots/upsert": {
      const byId = { ...state.snapshots.byId, [m.snapshot.id]: m.snapshot };
      const allIds = state.snapshots.allIds.includes(m.snapshot.id) ? state.snapshots.allIds : [...state.snapshots.allIds, m.snapshot.id];
      return { ...state, snapshots: { ...state.snapshots, byId, allIds } };
    }
    case "snapshots/setCreateBusy":
      return { ...state, snapshots: { ...state.snapshots, createBusy: m.busy } };
    case "snapshots/setLastResult":
      return { ...state, snapshots: { ...state.snapshots, lastResult: m.result } };
    case "restore/start": {
      return { ...state, restore: { ...state.restore, currentCommit: m.commit, steps: m.steps, progress: 0, errors: [], verification: { signatureValid: null, policyValid: null, attestationValid: null, verdict: "unknown" } } };
    }
    case "restore/updateStep": {
      const steps = state.restore.steps.map((s) => (s.id === m.stepId ? { ...s, status: m.status, message: m.message } : s));
      return { ...state, restore: { ...state.restore, steps } };
    }
    case "restore/setProgress":
      return { ...state, restore: { ...state.restore, progress: m.progress } };
    case "restore/addError":
      return { ...state, restore: { ...state.restore, errors: [...state.restore.errors, m.error] } };
    case "restore/setVerification": {
      const verification = { ...state.restore.verification, ...m.verification };
      if (verification.signatureValid === true && verification.policyValid === true && verification.attestationValid === true) {
        verification.verdict = "trusted";
      } else if (verification.signatureValid === false || verification.policyValid === false || verification.attestationValid === false) {
        verification.verdict = "untrusted";
      }
      return { ...state, restore: { ...state.restore, verification } };
    }
    case "restore/reset":
      return { ...state, restore: { currentCommit: null, steps: [], progress: 0, errors: [], verification: { signatureValid: null, policyValid: null, attestationValid: null, verdict: "unknown" } } };
    case "trust/setAppliance":
      return { ...state, trust: { ...state.trust, byAppliance: { ...state.trust.byAppliance, [m.record.applianceId]: m.record } } };
    case "trust/setSurface":
      return { ...state, trust: { ...state.trust, bySurface: { ...state.trust.bySurface, [m.surfaceId]: m.record } } };
    case "trust/setPolicy":
      return { ...state, trust: { ...state.trust, policy: { ...state.trust.policy, ...m.policy } } };
    case "trust/addKey":
      return { ...state, trust: { ...state.trust, keysRegistry: { ...state.trust.keysRegistry, [m.keyId]: m.key } } };
    case "events/push": {
      const buffer = [m.event, ...state.events.buffer].slice(0, state.events.bufferMaxSize);
      const index = { ...state.events.index };
      index.byType = { ...index.byType };
      if (!index.byType[m.event.type]) index.byType[m.event.type] = [];
      index.byType[m.event.type] = [m.event.id, ...index.byType[m.event.type]].slice(0, 100);
      if (m.event.applianceId) {
        index.byAppliance = { ...index.byAppliance };
        if (!index.byAppliance[m.event.applianceId]) index.byAppliance[m.event.applianceId] = [];
        index.byAppliance[m.event.applianceId] = [m.event.id, ...index.byAppliance[m.event.applianceId]].slice(0, 100);
      }
      return { ...state, events: { ...state.events, buffer, index, lastEventId: m.event.id } };
    }
    case "events/setConnected":
      return { ...state, events: { ...state.events, sseConnected: m.connected } };
    case "events/setLastEventId":
      return { ...state, events: { ...state.events, lastEventId: m.id } };
    case "render/patch": {
      const prev = state.render.channels[m.channelId];
      const patch = { ...(prev?.patch ?? {}), ...m.patch };
      return { ...state, render: { channels: { ...state.render.channels, [m.channelId]: { lastPatchAtMs: Date.now(), patch } } } };
    }
    default:
      return state;
  }
}

// ============================================================================
// Actions
// ============================================================================

export type AppActions = {
  setAuthStatus: (status: AuthStatus) => void;
  setToken: (token: string | null) => void;
  setAdminToken: (token: string | null) => void;
  setIdentity: (identity: AuthState["identity"]) => void;
  loginSuccess: (token: string, identity: AuthState["identity"]) => void;
  logout: () => void;
  pushToast: (toast: Omit<ToastItem, "id">) => string;
  dismissToast: (id: string) => void;
  toggleSidebar: () => void;
  openModal: (modalId: string) => void;
  closeModal: () => void;
  confirm: (dialog: Omit<ConfirmDialogState, "onConfirm" | "cancelLabel" | "tone"> & { onConfirm: () => void; cancelLabel?: string; tone?: ConfirmDialogState["tone"] }) => void;
  cancelConfirm: () => void;
  setApplianceFilters: (filters: Partial<AppliancesState["filters"]>) => void;
  selectAppliances: (ids: string[]) => void;
  toggleApplianceSelection: (id: string) => void;
  clearApplianceSelection: () => void;
  startGraphEdit: () => void;
  discardGraphDraft: () => void;
  updateGraphNode: (nodeId: string, updates: Partial<GraphNode>) => void;
  addGraphEdge: (edge: GraphEdge) => void;
  removeGraphEdge: (edgeId: string) => void;
  selectGraphNodes: (nodeIds: string[]) => void;
  selectGraphEdges: (edgeIds: string[]) => void;
  setSnapshotCreateBusy: (busy: boolean) => void;
  startRestore: (commit: string) => void;
  resetRestore: () => void;
  patchRender: (channelId: string, patch: Record<string, unknown>) => void;
};

type StoreValue = {
  state: AppState;
  commit: (m: Mutation) => void;
  actions: AppActions;
};

const StoreContext = React.createContext<StoreValue | null>(null);

export function StoreProvider({ children }: React.PropsWithChildren) {
  const [state, dispatch] = useReducer(reducer, undefined, createInitialState);
  const commit = dispatch;

  useEffect(() => {
    if (typeof document === "undefined") return;
    const handler = () => commit({ type: "ui/setVisibility", isVisible: document.visibilityState !== "hidden" });
    document.addEventListener("visibilitychange", handler);
    return () => document.removeEventListener("visibilitychange", handler);
  }, []);

  const actions: AppActions = useMemo(
    () => ({
      setAuthStatus: (status) => commit({ type: "auth/setStatus", status }),
      setToken: (token) => commit({ type: "auth/setToken", token }),
      setAdminToken: (token) => commit({ type: "auth/setAdminToken", token }),
      setIdentity: (identity) => commit({ type: "auth/setIdentity", identity }),
      loginSuccess: (token, identity) => {
        commit({ type: "auth/setToken", token });
        commit({ type: "auth/setIdentity", identity });
        commit({ type: "auth/setStatus", status: "authenticated" });
      },
      logout: () => {
        commit({ type: "auth/setToken", token: null });
        commit({ type: "auth/setAdminToken", token: null });
        commit({ type: "auth/setViewToken", token: null });
        commit({ type: "auth/setCapabilities", capabilities: [], expiry: 0 });
        commit({ type: "auth/setIdentity", identity: null });
        commit({ type: "auth/setStatus", status: "unauthenticated" });
      },
      pushToast: (toast) => {
        const id = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
        commit({ type: "ui/pushToast", toast: { id, ...toast } });
        return id;
      },
      dismissToast: (id) => commit({ type: "ui/dismissToast", id }),
      toggleSidebar: () => commit({ type: "ui/toggleSidebar" }),
      openModal: (modalId) => commit({ type: "ui/setModal", modalId }),
      closeModal: () => commit({ type: "ui/setModal", modalId: null }),
      confirm: (dialog) => commit({ type: "ui/showConfirm", dialog: { ...dialog, cancelLabel: dialog.cancelLabel ?? "Cancel", tone: dialog.tone ?? "warning" } }),
      cancelConfirm: () => commit({ type: "ui/hideConfirm" }),
      setApplianceFilters: (filters) => commit({ type: "appliances/setFilters", filters }),
      selectAppliances: (ids) => commit({ type: "appliances/setSelection", ids }),
      toggleApplianceSelection: (id) => commit({ type: "appliances/toggleSelection", id }),
      clearApplianceSelection: () => commit({ type: "appliances/setSelection", ids: [] }),
      startGraphEdit: () => {
        if (state.graph.current) {
          commit({ type: "graph/setDraft", draft: { nodes: [...state.graph.current.nodes], edges: [...state.graph.current.edges] } });
        }
      },
      discardGraphDraft: () => commit({ type: "graph/setDraft", draft: null }),
      updateGraphNode: (nodeId, updates) => commit({ type: "graph/updateDraftNode", nodeId, updates }),
      addGraphEdge: (edge) => commit({ type: "graph/addDraftEdge", edge }),
      removeGraphEdge: (edgeId) => commit({ type: "graph/removeDraftEdge", edgeId }),
      selectGraphNodes: (nodeIds) => commit({ type: "graph/setSelection", selection: { nodes: nodeIds, edges: state.graph.selection.edges } }),
      selectGraphEdges: (edgeIds) => commit({ type: "graph/setSelection", selection: { nodes: state.graph.selection.nodes, edges: edgeIds } }),
      setSnapshotCreateBusy: (busy) => commit({ type: "snapshots/setCreateBusy", busy }),
      startRestore: (commitHash) => {
        const steps: RestoreStep[] = [
          { id: "verify", label: "Verify snapshot", status: "pending" },
          { id: "plan", label: "Generate restore plan", status: "pending" },
          { id: "execute", label: "Execute restore", status: "pending" },
          { id: "health", label: "Health check", status: "pending" },
        ];
        commit({ type: "restore/start", commit: commitHash, steps });
      },
      resetRestore: () => commit({ type: "restore/reset" }),
      patchRender: (channelId, patch) => commit({ type: "render/patch", channelId, patch }),
    }),
    [state.graph.current, state.graph.selection]
  );

  const value = useMemo<StoreValue>(() => ({ state, commit, actions }), [state, actions]);
  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore() {
  const v = useContext(StoreContext);
  if (!v) throw new Error("StoreProvider missing");
  return v;
}

export function useSelector<T>(selector: (s: AppState) => T): T {
  const { state } = useStore();
  return selector(state);
}

export function useActions() {
  const { actions } = useStore();
  return actions;
}

export function useCommit() {
  const { commit } = useStore();
  return commit;
}

// ============================================================================
// Derived Selectors
// ============================================================================

export function useFilteredAppliances() {
  const { state } = useStore();
  const { byId, allIds, filters } = state.appliances;
  return useMemo(() => {
    let ids = allIds;
    if (filters.status.length > 0) ids = ids.filter((id) => filters.status.includes(byId[id]?.status ?? ""));
    if (filters.templateId.length > 0) ids = ids.filter((id) => filters.templateId.includes(byId[id]?.templateId ?? ""));
    if (filters.search.trim()) {
      const q = filters.search.toLowerCase();
      ids = ids.filter((id) => {
        const a = byId[id];
        return a?.name.toLowerCase().includes(q) || a?.id.toLowerCase().includes(q);
      });
    }
    return ids.map((id) => byId[id]).filter(Boolean) as ApplianceRecord[];
  }, [byId, allIds, filters]);
}

export function useApplianceById(id: string | undefined) {
  const { state } = useStore();
  return id ? state.appliances.byId[id] : undefined;
}

export function useTemplateById(id: string | undefined) {
  const { state } = useStore();
  return id ? state.templates.byId[id] : undefined;
}

export function useApplianceTrust(applianceId: string | undefined) {
  const { state } = useStore();
  return applianceId ? state.trust.byAppliance[applianceId] : undefined;
}

export function useCapability(capability: string) {
  const { state } = useStore();
  return state.auth.capabilities.includes(capability) || state.auth.capabilities.includes("*");
}

export function useRecentEvents(limit = 20) {
  const { state } = useStore();
  return state.events.buffer.slice(0, limit);
}

export function useEventsByAppliance(applianceId: string | undefined, limit = 20) {
  const { state } = useStore();
  if (!applianceId) return [];
  const ids = state.events.index.byAppliance[applianceId] ?? [];
  return ids.slice(0, limit).map((id) => state.events.buffer.find((e) => e.id === id)).filter(Boolean) as AuditEvent[];
}

export function useRenderChannel(channelId: string) {
  const { state, actions } = useStore();
  const channel = state.render.channels[channelId];
  return {
    patch: channel?.patch ?? {},
    lastPatchAtMs: channel?.lastPatchAtMs ?? 0,
    pushPatch: (patch: Record<string, unknown>) => actions.patchRender(channelId, patch),
  };
}
