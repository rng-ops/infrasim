/**
 * Meshnet Console MVP - Dashboard
 * 
 * Single-page UI with 3 top-level cards:
 * 1. Identity - Handle and provisioning status
 * 2. Mesh - WireGuard peers and configs
 * 3. Appliances - Downloadable archives
 */
import React, { useEffect, useState } from "react";
import { Button, Card, PageHeader, StatusChip, CodeBlock } from "@infrasim/ui";

// =============================================================================
// Types
// =============================================================================

interface MeshnetUser {
  id: string;
  created_at: number;
}

interface MeshnetIdentity {
  id: string;
  user_id: string;
  handle: string;
  fqdn: string;
  matrix_id: string;
  status_subdomain: string;
  status_matrix: string;
  status_storage: string;
  created_at: number;
}

interface ProvisioningStatus {
  subdomain: { state: string; error: string | null };
  matrix: { state: string; error: string | null };
  storage: { state: string; error: string | null };
  overall: string;
}

interface MeResponse {
  user: MeshnetUser | null;
  identity: MeshnetIdentity | null;
  statuses: ProvisioningStatus | null;
}

interface MeshPeer {
  id: string;
  identity_id: string;
  name: string;
  public_key: string;
  private_key: string;
  address: string;
  status: string;
  created_at: number;
}

interface MeshnetAppliance {
  id: string;
  identity_id: string;
  name: string;
  version: string;
  archive_path: string | null;
  status: string;
  created_at: number;
}

// =============================================================================
// API helpers
// =============================================================================

const API_BASE = "/api/meshnet";

async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const token = sessionStorage.getItem("meshnet.token");
  const res = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(options?.headers || {}),
    },
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: "Request failed" }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

// =============================================================================
// Components
// =============================================================================

function ProvisioningBadge({ status }: { status: string }) {
  const toneMap: Record<string, "success" | "warning" | "danger" | "muted"> = {
    active: "success",
    pending: "warning",
    failed: "danger",
    not_started: "muted",
  };
  return <StatusChip tone={toneMap[status] || "muted"} label={status} />;
}

// Identity Card
function IdentityCard({ 
  identity, 
  statuses,
  onCreateIdentity,
  onStartProvisioning,
  busy,
  error,
}: {
  identity: MeshnetIdentity | null;
  statuses: ProvisioningStatus | null;
  onCreateIdentity: (handle: string) => void;
  onStartProvisioning: () => void;
  busy: boolean;
  error: string | null;
}) {
  const [handle, setHandle] = useState("");
  const [showCreate, setShowCreate] = useState(false);

  if (!identity) {
    if (!showCreate) {
      return (
        <Card title="Identity" actions={null}>
          <p>No identity configured. Create one to get started.</p>
          <Button onClick={() => setShowCreate(true)}>Create Identity</Button>
        </Card>
      );
    }

    return (
      <Card title="Create Identity" actions={null}>
        <form onSubmit={e => { e.preventDefault(); onCreateIdentity(handle); }}>
          <div style={{ marginBottom: "1rem" }}>
            <label htmlFor="handle" style={{ display: "block", marginBottom: "0.5rem" }}>
              Handle (3-32 lowercase letters, numbers, hyphens)
            </label>
            <input
              id="handle"
              type="text"
              value={handle}
              onChange={e => setHandle(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))}
              placeholder="myhandle"
              maxLength={32}
              style={{ 
                width: "100%", 
                padding: "0.5rem", 
                borderRadius: "4px",
                border: "1px solid var(--ifm-color-border)",
                background: "var(--ifm-color-surface)",
                color: "var(--ifm-color-text)",
              }}
            />
          </div>
          {error && <p style={{ color: "var(--ifm-color-danger)", marginBottom: "1rem" }}>{error}</p>}
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <Button type="submit" disabled={busy || handle.length < 3}>
              {busy ? "Creating..." : "Create"}
            </Button>
            <Button variant="ghost" onClick={() => setShowCreate(false)}>Cancel</Button>
          </div>
        </form>
      </Card>
    );
  }

  return (
    <Card 
      title="Identity" 
      actions={
        statuses?.overall !== "active" && statuses?.overall !== "pending" ? (
          <Button variant="ghost" onClick={onStartProvisioning} disabled={busy}>
            {busy ? "..." : "Start Provisioning"}
          </Button>
        ) : null
      }
    >
      <div className="identity-details">
        <div className="detail-row">
          <span className="label">Handle</span>
          <span className="value">{identity.handle}</span>
        </div>
        <div className="detail-row">
          <span className="label">Domain</span>
          <span className="value">{identity.fqdn}</span>
        </div>
        <div className="detail-row">
          <span className="label">Matrix ID</span>
          <span className="value">{identity.matrix_id}</span>
        </div>
        
        {statuses && (
          <>
            <h4 style={{ marginTop: "1rem", marginBottom: "0.5rem" }}>Provisioning Status</h4>
            <div className="detail-row">
              <span className="label">Overall</span>
              <ProvisioningBadge status={statuses.overall} />
            </div>
            <div className="detail-row">
              <span className="label">Subdomain</span>
              <ProvisioningBadge status={statuses.subdomain.state} />
            </div>
            <div className="detail-row">
              <span className="label">Matrix</span>
              <ProvisioningBadge status={statuses.matrix.state} />
            </div>
            <div className="detail-row">
              <span className="label">Storage</span>
              <ProvisioningBadge status={statuses.storage.state} />
            </div>
          </>
        )}
      </div>
      {error && <p style={{ color: "var(--ifm-color-danger)", marginTop: "1rem" }}>{error}</p>}
    </Card>
  );
}

// Mesh Card
function MeshCard({
  peers,
  onCreatePeer,
  onDownloadConfig,
  busy,
  error,
}: {
  peers: MeshPeer[];
  onCreatePeer: (name: string) => void;
  onDownloadConfig: (peerId: string) => void;
  busy: boolean;
  error: string | null;
}) {
  const [peerName, setPeerName] = useState("");
  const [showCreate, setShowCreate] = useState(false);

  return (
    <Card 
      title="Mesh Network" 
      actions={
        !showCreate ? (
          <Button variant="ghost" onClick={() => setShowCreate(true)}>Add Peer</Button>
        ) : null
      }
    >
      {showCreate && (
        <form onSubmit={e => { e.preventDefault(); onCreatePeer(peerName); setPeerName(""); setShowCreate(false); }} style={{ marginBottom: "1rem" }}>
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <input
              type="text"
              value={peerName}
              onChange={e => setPeerName(e.target.value)}
              placeholder="Peer name (e.g., laptop, phone)"
              style={{ 
                flex: 1,
                padding: "0.5rem", 
                borderRadius: "4px",
                border: "1px solid var(--ifm-color-border)",
                background: "var(--ifm-color-surface)",
                color: "var(--ifm-color-text)",
              }}
            />
            <Button type="submit" disabled={busy || !peerName.trim()}>
              {busy ? "..." : "Add"}
            </Button>
            <Button variant="ghost" onClick={() => setShowCreate(false)}>Cancel</Button>
          </div>
        </form>
      )}

      {peers.length === 0 ? (
        <p>No peers configured. Add a peer to generate WireGuard configs.</p>
      ) : (
        <div className="peer-list">
          {peers.map(peer => (
            <div key={peer.id} className="peer-row">
              <div className="peer-info">
                <span className="peer-name">{peer.name}</span>
                <span className="peer-address">{peer.address}</span>
              </div>
              <div className="peer-actions">
                <StatusChip tone={peer.status === "active" ? "success" : "muted"} label={peer.status} />
                <Button 
                  variant="ghost" 
                  onClick={() => onDownloadConfig(peer.id)}
                  aria-label={`Download config for ${peer.name}`}
                >
                  Download
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
      {error && <p style={{ color: "var(--ifm-color-danger)", marginTop: "1rem" }}>{error}</p>}
    </Card>
  );
}

// Appliances Card
function AppliancesCard({
  appliances,
  onCreateAppliance,
  onDownloadArchive,
  busy,
  error,
}: {
  appliances: MeshnetAppliance[];
  onCreateAppliance: (name: string) => void;
  onDownloadArchive: (applianceId: string) => void;
  busy: boolean;
  error: string | null;
}) {
  const [name, setName] = useState("");
  const [showCreate, setShowCreate] = useState(false);

  return (
    <Card 
      title="Appliances" 
      actions={
        !showCreate ? (
          <Button variant="ghost" onClick={() => setShowCreate(true)}>Create Appliance</Button>
        ) : null
      }
    >
      {showCreate && (
        <form onSubmit={e => { e.preventDefault(); onCreateAppliance(name); setName(""); setShowCreate(false); }} style={{ marginBottom: "1rem" }}>
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <input
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="Appliance name"
              style={{ 
                flex: 1,
                padding: "0.5rem", 
                borderRadius: "4px",
                border: "1px solid var(--ifm-color-border)",
                background: "var(--ifm-color-surface)",
                color: "var(--ifm-color-text)",
              }}
            />
            <Button type="submit" disabled={busy || !name.trim()}>
              {busy ? "..." : "Create"}
            </Button>
            <Button variant="ghost" onClick={() => setShowCreate(false)}>Cancel</Button>
          </div>
        </form>
      )}

      {appliances.length === 0 ? (
        <p>No appliances created. Create one to generate a downloadable archive.</p>
      ) : (
        <div className="appliance-list">
          {appliances.map(app => (
            <div key={app.id} className="appliance-row">
              <div className="appliance-info">
                <span className="appliance-name">{app.name}</span>
                <span className="appliance-version">v{app.version}</span>
              </div>
              <div className="appliance-actions">
                <StatusChip 
                  tone={app.status === "ready" ? "success" : app.status === "building" ? "warning" : "muted"} 
                  label={app.status} 
                />
                {app.status === "ready" && (
                  <Button 
                    variant="ghost" 
                    onClick={() => onDownloadArchive(app.id)}
                    aria-label={`Download ${app.name}`}
                  >
                    Download Archive
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
      {error && <p style={{ color: "var(--ifm-color-danger)", marginTop: "1rem" }}>{error}</p>}
    </Card>
  );
}

// =============================================================================
// Main Dashboard
// =============================================================================

export default function MeshnetDashboard() {
  const [me, setMe] = useState<MeResponse | null>(null);
  const [peers, setPeers] = useState<MeshPeer[]>([]);
  const [appliances, setAppliances] = useState<MeshnetAppliance[]>([]);
  const [loading, setLoading] = useState(true);
  
  const [identityBusy, setIdentityBusy] = useState(false);
  const [identityError, setIdentityError] = useState<string | null>(null);
  
  const [meshBusy, setMeshBusy] = useState(false);
  const [meshError, setMeshError] = useState<string | null>(null);
  
  const [applianceBusy, setApplianceBusy] = useState(false);
  const [applianceError, setApplianceError] = useState<string | null>(null);

  // Fetch current state
  const fetchData = async () => {
    try {
      const meData = await api<MeResponse>("/me");
      setMe(meData);
      
      if (meData.identity) {
        const peersData = await api<MeshPeer[]>("/mesh/peers");
        setPeers(peersData);
        
        const appData = await api<MeshnetAppliance[]>("/appliances");
        setAppliances(appData);
      }
    } catch (err) {
      console.error("Failed to fetch data:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchData();
  }, []);

  // Identity handlers
  const handleCreateIdentity = async (handle: string) => {
    setIdentityBusy(true);
    setIdentityError(null);
    try {
      await api("/identity", {
        method: "POST",
        body: JSON.stringify({ handle }),
      });
      await fetchData();
    } catch (err: any) {
      setIdentityError(err.message);
    } finally {
      setIdentityBusy(false);
    }
  };

  const handleStartProvisioning = async () => {
    setIdentityBusy(true);
    setIdentityError(null);
    try {
      await api("/identity/provision", { method: "POST" });
      await fetchData();
    } catch (err: any) {
      setIdentityError(err.message);
    } finally {
      setIdentityBusy(false);
    }
  };

  // Mesh handlers
  const handleCreatePeer = async (name: string) => {
    setMeshBusy(true);
    setMeshError(null);
    try {
      await api("/mesh/peers", {
        method: "POST",
        body: JSON.stringify({ name }),
      });
      const peersData = await api<MeshPeer[]>("/mesh/peers");
      setPeers(peersData);
    } catch (err: any) {
      setMeshError(err.message);
    } finally {
      setMeshBusy(false);
    }
  };

  const handleDownloadConfig = async (peerId: string) => {
    // Trigger download
    const token = sessionStorage.getItem("meshnet.token");
    const res = await fetch(`${API_BASE}/mesh/peers/${peerId}/config`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!res.ok) {
      setMeshError("Failed to download config");
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `wireguard-${peerId}.conf`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // Appliance handlers
  const handleCreateAppliance = async (name: string) => {
    setApplianceBusy(true);
    setApplianceError(null);
    try {
      await api("/appliances", {
        method: "POST",
        body: JSON.stringify({ name }),
      });
      const appData = await api<MeshnetAppliance[]>("/appliances");
      setAppliances(appData);
    } catch (err: any) {
      setApplianceError(err.message);
    } finally {
      setApplianceBusy(false);
    }
  };

  const handleDownloadArchive = async (applianceId: string) => {
    const token = sessionStorage.getItem("meshnet.token");
    const res = await fetch(`${API_BASE}/appliances/${applianceId}/archive`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!res.ok) {
      setApplianceError("Failed to download archive");
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `appliance-${applianceId}.tar.gz`;
    a.click();
    URL.revokeObjectURL(url);
  };

  if (loading) {
    return (
      <div>
        <PageHeader title="Meshnet Console" description="Loading..." />
        <div style={{ textAlign: "center", padding: "2rem" }}>
          <div className="spinner" style={{ 
            width: 40, height: 40, 
            border: "3px solid var(--ifm-color-border)",
            borderTopColor: "var(--ifm-color-accent)",
            borderRadius: "50%",
            animation: "spin 0.8s linear infinite",
            margin: "0 auto"
          }} />
        </div>
      </div>
    );
  }

  return (
    <div className="meshnet-dashboard">
      <PageHeader 
        title="Meshnet Console" 
        description={me?.identity ? `Welcome, ${me.identity.handle}` : "Set up your mesh identity"}
        actions={<Button variant="secondary" onClick={fetchData}>Refresh</Button>}
      />
      
      <div className="dashboard-grid">
        <IdentityCard
          identity={me?.identity || null}
          statuses={me?.statuses || null}
          onCreateIdentity={handleCreateIdentity}
          onStartProvisioning={handleStartProvisioning}
          busy={identityBusy}
          error={identityError}
        />
        
        {me?.identity && (
          <>
            <MeshCard
              peers={peers}
              onCreatePeer={handleCreatePeer}
              onDownloadConfig={handleDownloadConfig}
              busy={meshBusy}
              error={meshError}
            />
            
            <AppliancesCard
              appliances={appliances}
              onCreateAppliance={handleCreateAppliance}
              onDownloadArchive={handleDownloadArchive}
              busy={applianceBusy}
              error={applianceError}
            />
          </>
        )}
      </div>
      
      <style>{`
        .meshnet-dashboard { max-width: 1200px; }
        .dashboard-grid { 
          display: grid; 
          grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); 
          gap: 1.5rem; 
        }
        .identity-details, .peer-list, .appliance-list { 
          display: flex; 
          flex-direction: column; 
          gap: 0.5rem; 
        }
        .detail-row { 
          display: flex; 
          justify-content: space-between; 
          align-items: center;
          padding: 0.25rem 0;
        }
        .detail-row .label { 
          color: var(--ifm-color-subtle); 
          font-size: 0.875rem;
        }
        .detail-row .value { 
          font-family: monospace; 
          font-size: 0.875rem;
        }
        .peer-row, .appliance-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 0.75rem;
          background: var(--ifm-color-surface-muted);
          border-radius: 4px;
        }
        .peer-info, .appliance-info {
          display: flex;
          flex-direction: column;
          gap: 0.25rem;
        }
        .peer-name, .appliance-name {
          font-weight: 500;
        }
        .peer-address, .appliance-version {
          font-size: 0.75rem;
          color: var(--ifm-color-subtle);
          font-family: monospace;
        }
        .peer-actions, .appliance-actions {
          display: flex;
          align-items: center;
          gap: 0.5rem;
        }
        @keyframes spin { to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}
