import React, { useEffect, useState } from "react";
import { Button, Card, PageHeader } from "@infrasim/ui";

interface MdmStatus {
  initialized: boolean;
  org_name: string;
  domain: string;
  bridge_count: number;
  vpn_count: number;
  cert_store_path: string;
}

interface Bridge {
  name: string;
  subnet: string;
  gateway: string;
  dns_servers: string[];
}

interface Vpn {
  display_name: string;
  server: string;
  vpn_type: string;
  on_demand: boolean;
}

export default function Config() {
  const [status, setStatus] = useState<MdmStatus | null>(null);
  const [bridges, setBridges] = useState<Bridge[]>([]);
  const [vpns, setVpns] = useState<Vpn[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // New bridge form
  const [newBridge, setNewBridge] = useState({
    name: "",
    subnet: "10.0.0.0/24",
    gateway: "10.0.0.1",
    dns_servers: "8.8.8.8,8.8.4.4",
  });

  // New VPN form
  const [newVpn, setNewVpn] = useState({
    display_name: "",
    server: "",
    vpn_type: "ikev2",
    shared_secret: "",
    on_demand: true,
    trusted_ssids: "",
  });

  const [profileName, setProfileName] = useState("InfraSim");
  const [profileUrl, setProfileUrl] = useState<string | null>(null);

  const fetchStatus = async () => {
    try {
      const resp = await fetch("/api/mdm/status");
      if (resp.ok) {
        setStatus(await resp.json());
      }
    } catch (e) {
      console.error("Failed to fetch MDM status", e);
    }
  };

  const fetchBridges = async () => {
    try {
      const resp = await fetch("/api/mdm/bridges");
      if (resp.ok) {
        const data = await resp.json();
        setBridges(data.bridges || []);
      }
    } catch (e) {
      console.error("Failed to fetch bridges", e);
    }
  };

  const fetchVpns = async () => {
    try {
      const resp = await fetch("/api/mdm/vpns");
      if (resp.ok) {
        const data = await resp.json();
        setVpns(data.vpns || []);
      }
    } catch (e) {
      console.error("Failed to fetch VPNs", e);
    }
  };

  useEffect(() => {
    Promise.all([fetchStatus(), fetchBridges(), fetchVpns()]).finally(() =>
      setLoading(false)
    );
  }, []);

  const addBridge = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const resp = await fetch("/api/mdm/bridges", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: newBridge.name,
          subnet: newBridge.subnet,
          gateway: newBridge.gateway,
          dns_servers: newBridge.dns_servers.split(",").map((s) => s.trim()),
        }),
      });
      if (!resp.ok) throw new Error("Failed to add bridge");
      await fetchBridges();
      await fetchStatus();
      setNewBridge({ name: "", subnet: "10.0.0.0/24", gateway: "10.0.0.1", dns_servers: "8.8.8.8,8.8.4.4" });
    } catch (e: any) {
      setError(e.message);
    }
  };

  const addVpn = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      const resp = await fetch("/api/mdm/vpns", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          display_name: newVpn.display_name,
          server: newVpn.server,
          vpn_type: newVpn.vpn_type,
          shared_secret: newVpn.shared_secret || null,
          on_demand: newVpn.on_demand,
          trusted_ssids: newVpn.trusted_ssids.split(",").map((s) => s.trim()).filter(Boolean),
        }),
      });
      if (!resp.ok) throw new Error("Failed to add VPN");
      await fetchVpns();
      await fetchStatus();
      setNewVpn({ display_name: "", server: "", vpn_type: "ikev2", shared_secret: "", on_demand: true, trusted_ssids: "" });
    } catch (e: any) {
      setError(e.message);
    }
  };

  const generateProfile = async () => {
    setError(null);
    try {
      const resp = await fetch("/api/mdm/profile", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: profileName }),
      });
      if (!resp.ok) throw new Error("Failed to generate profile");
      const data = await resp.json();
      setProfileUrl(data.download_url);
    } catch (e: any) {
      setError(e.message);
    }
  };

  if (loading) {
    return (
      <div className="dashboard-page">
        <PageHeader title="Configuration" description="Loading..." />
      </div>
    );
  }

  return (
    <div className="dashboard-page">
      <PageHeader
        title="MDM Configuration"
        description="Set up bridge networks, VPN endpoints, and generate signed .mobileconfig profiles for iOS/macOS devices"
      />

      {error && (
        <div className="login-error" style={{ marginBottom: "1rem" }}>
          {error}
        </div>
      )}

      <div className="card-grid">
        {/* Status Card */}
        <Card>
          <h3>MDM Status</h3>
          {status ? (
            <div style={{ fontSize: "0.875rem", color: "var(--ifm-color-subtle)" }}>
              <p><strong>Organization:</strong> {status.org_name}</p>
              <p><strong>Domain:</strong> {status.domain}</p>
              <p><strong>Initialized:</strong> {status.initialized ? "Yes ✓" : "No"}</p>
              <p><strong>Bridges:</strong> {status.bridge_count}</p>
              <p><strong>VPNs:</strong> {status.vpn_count}</p>
              <div style={{ marginTop: "1rem" }}>
                <a
                  href="/api/mdm/root-ca"
                  download="infrasim-root-ca.crt"
                  style={{ color: "var(--ifm-color-accent)" }}
                >
                  Download Root CA Certificate
                </a>
              </div>
            </div>
          ) : (
            <p>Failed to load status</p>
          )}
        </Card>

        {/* Bridge Networks */}
        <Card>
          <h3>Bridge Networks</h3>
          {bridges.length === 0 ? (
            <p style={{ color: "var(--ifm-color-subtle)", fontSize: "0.875rem" }}>
              No bridges configured
            </p>
          ) : (
            <ul style={{ fontSize: "0.875rem", marginBottom: "1rem" }}>
              {bridges.map((b, i) => (
                <li key={i}>
                  <strong>{b.name}</strong> - {b.subnet} (GW: {b.gateway})
                </li>
              ))}
            </ul>
          )}
          <form onSubmit={addBridge} style={{ marginTop: "1rem" }}>
            <input
              placeholder="Network name"
              value={newBridge.name}
              onChange={(e) => setNewBridge({ ...newBridge, name: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <input
              placeholder="Subnet (e.g., 10.0.0.0/24)"
              value={newBridge.subnet}
              onChange={(e) => setNewBridge({ ...newBridge, subnet: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <input
              placeholder="Gateway"
              value={newBridge.gateway}
              onChange={(e) => setNewBridge({ ...newBridge, gateway: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <input
              placeholder="DNS servers (comma-separated)"
              value={newBridge.dns_servers}
              onChange={(e) => setNewBridge({ ...newBridge, dns_servers: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <Button type="submit" disabled={!newBridge.name}>
              Add Bridge
            </Button>
          </form>
        </Card>

        {/* VPN Endpoints */}
        <Card>
          <h3>VPN Endpoints</h3>
          {vpns.length === 0 ? (
            <p style={{ color: "var(--ifm-color-subtle)", fontSize: "0.875rem" }}>
              No VPNs configured
            </p>
          ) : (
            <ul style={{ fontSize: "0.875rem", marginBottom: "1rem" }}>
              {vpns.map((v, i) => (
                <li key={i}>
                  <strong>{v.display_name}</strong> - {v.server} ({v.vpn_type})
                </li>
              ))}
            </ul>
          )}
          <form onSubmit={addVpn} style={{ marginTop: "1rem" }}>
            <input
              placeholder="VPN name"
              value={newVpn.display_name}
              onChange={(e) => setNewVpn({ ...newVpn, display_name: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <input
              placeholder="Server (e.g., vpn.example.com)"
              value={newVpn.server}
              onChange={(e) => setNewVpn({ ...newVpn, server: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <select
              value={newVpn.vpn_type}
              onChange={(e) => setNewVpn({ ...newVpn, vpn_type: e.target.value })}
              style={{ marginBottom: "0.5rem", width: "100%" }}
            >
              <option value="ikev2">IKEv2</option>
              <option value="ipsec">IPSec</option>
              <option value="wireguard">WireGuard</option>
            </select>
            <input
              placeholder="Shared secret (optional)"
              type="password"
              value={newVpn.shared_secret}
              onChange={(e) => setNewVpn({ ...newVpn, shared_secret: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <input
              placeholder="Trusted SSIDs (comma-separated, optional)"
              value={newVpn.trusted_ssids}
              onChange={(e) => setNewVpn({ ...newVpn, trusted_ssids: e.target.value })}
              style={{ marginBottom: "0.5rem" }}
            />
            <label style={{ display: "flex", alignItems: "center", gap: "0.5rem", marginBottom: "0.5rem" }}>
              <input
                type="checkbox"
                checked={newVpn.on_demand}
                onChange={(e) => setNewVpn({ ...newVpn, on_demand: e.target.checked })}
              />
              Connect on demand
            </label>
            <Button type="submit" disabled={!newVpn.display_name || !newVpn.server}>
              Add VPN
            </Button>
          </form>
        </Card>

        {/* Generate Profile */}
        <Card>
          <h3>Generate Profile</h3>
          <p style={{ color: "var(--ifm-color-subtle)", fontSize: "0.875rem", marginBottom: "1rem" }}>
            Generate a .mobileconfig profile containing all configured bridges and VPNs.
            Install on iOS/macOS devices.
          </p>
          <input
            placeholder="Profile name"
            value={profileName}
            onChange={(e) => setProfileName(e.target.value)}
            style={{ marginBottom: "0.5rem" }}
          />
          <Button onClick={generateProfile} disabled={!profileName}>
            Generate Profile
          </Button>

          {profileUrl && (
            <div style={{ marginTop: "1rem" }}>
              <a
                href={profileUrl}
                download
                style={{
                  display: "inline-block",
                  padding: "0.5rem 1rem",
                  background: "var(--ifm-color-success)",
                  color: "white",
                  borderRadius: "var(--ifm-radius-sm)",
                  textDecoration: "none",
                }}
              >
                ⬇ Download .mobileconfig
              </a>
            </div>
          )}

          <div style={{ marginTop: "1rem", fontSize: "0.75rem", color: "var(--ifm-color-subtle)" }}>
            <strong>Webhook URL:</strong>
            <code style={{ display: "block", marginTop: "0.25rem", wordBreak: "break-all" }}>
              {window.location.origin}/webhook/config/YOUR_TOKEN_HERE
            </code>
          </div>
        </Card>
      </div>
    </div>
  );
}
