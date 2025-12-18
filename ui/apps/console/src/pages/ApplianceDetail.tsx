import React, { useState, useEffect, useRef } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Tabs,
  TabItem,
  StatusChip,
  PropertyGrid,
  PropertyRow,
  Timeline,
  TimelineItem,
  DockLayout,
  Panel,
  Toolbar,
  ToolbarSpacer,
  ToolbarDivider,
  TrustBadge,
  ConfirmDialog,
  Spinner,
  SurfaceHost,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Lifecycle Control Buttons
// ============================================================================

interface LifecycleControlsProps {
  status: string;
  onStart: () => void;
  onStop: () => void;
  onReboot: () => void;
  onDelete: () => void;
  busy: boolean;
}

function LifecycleControls({ status, onStart, onStop, onReboot, onDelete, busy }: LifecycleControlsProps) {
  const canStart = status === "stopped" || status === "error";
  const canStop = status === "running";
  const canReboot = status === "running";

  return (
    <Toolbar>
      <Button variant="primary" size="sm" onClick={onStart} disabled={!canStart || busy} loading={busy}>
        Start
      </Button>
      <Button variant="secondary" size="sm" onClick={onStop} disabled={!canStop || busy}>
        Stop
      </Button>
      <Button variant="secondary" size="sm" onClick={onReboot} disabled={!canReboot || busy}>
        Reboot
      </Button>
      <ToolbarDivider />
      <Button variant="ghost" size="sm" onClick={onDelete} disabled={busy}>
        Delete
      </Button>
    </Toolbar>
  );
}

// ============================================================================
// Console Surface (placeholder for VNC/SPICE)
// ============================================================================

interface ConsoleSurfaceProps {
  applianceId: string;
  connected: boolean;
}

function ConsoleSurface({ applianceId, connected }: ConsoleSurfaceProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    // Placeholder: would initialize VNC/SPICE connection here
    const canvas = canvasRef.current;
    if (canvas) {
      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.fillStyle = "#1e1e1e";
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.fillStyle = "#888";
        ctx.font = "14px monospace";
        ctx.textAlign = "center";
        ctx.fillText(connected ? "Console connected" : "Console disconnected", canvas.width / 2, canvas.height / 2);
      }
    }
  }, [connected]);

  return (
    <SurfaceHost connectionStatus={<StatusChip status={connected ? "success" : "muted"}>{connected ? "Connected" : "Disconnected"}</StatusChip>}>
      <canvas ref={canvasRef} width={800} height={600} style={{ width: "100%", height: "auto", background: "#1e1e1e", borderRadius: 4 }} />
    </SurfaceHost>
  );
}

// ============================================================================
// Serial Console (xterm-like)
// ============================================================================

interface SerialConsoleProps {
  applianceId: string;
}

function SerialConsole({ applianceId }: SerialConsoleProps) {
  const [output, setOutput] = useState<string[]>([
    "Serial console connected",
    "$ ",
  ]);
  const [input, setInput] = useState("");
  const outputRef = useRef<HTMLDivElement>(null);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      setOutput((prev) => [...prev, `$ ${input}`, `(command '${input}' executed)`]);
      setInput("");
    }
  };

  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [output]);

  return (
    <div style={{ background: "#0d0d0d", borderRadius: 4, padding: 8, fontFamily: "monospace", fontSize: 13 }}>
      <div ref={outputRef} style={{ height: 400, overflow: "auto", color: "#0f0" }}>
        {output.map((line, i) => (
          <div key={i}>{line}</div>
        ))}
      </div>
      <input
        type="text"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        style={{ width: "100%", background: "transparent", border: "none", color: "#0f0", outline: "none", fontFamily: "inherit", fontSize: "inherit" }}
        placeholder="Type command..."
        aria-label="Serial console input"
      />
    </div>
  );
}

// ============================================================================
// Attachments Panel
// ============================================================================

interface Attachment {
  id: string;
  type: "disk" | "network" | "usb";
  name: string;
  details: string;
}

function AttachmentPanel({ attachments }: { attachments: Attachment[] }) {
  const grouped = {
    disk: attachments.filter((a) => a.type === "disk"),
    network: attachments.filter((a) => a.type === "network"),
    usb: attachments.filter((a) => a.type === "usb"),
  };

  return (
    <div>
      {Object.entries(grouped).map(([type, items]) => (
        items.length > 0 && (
          <div key={type} style={{ marginBottom: 16 }}>
            <h5 style={{ textTransform: "capitalize", marginBottom: 8 }}>{type}s</h5>
            {items.map((a) => (
              <Card key={a.id} style={{ padding: 8, marginBottom: 4 }}>
                <div style={{ fontWeight: 500 }}>{a.name}</div>
                <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>{a.details}</div>
              </Card>
            ))}
          </div>
        )
      ))}
      {attachments.length === 0 && <p style={{ color: "var(--ifm-color-subtle)" }}>No attachments</p>}
    </div>
  );
}

// ============================================================================
// Main Detail Component
// ============================================================================

export function ApplianceDetail() {
  const { workspaceId, applianceId } = useParams<{ workspaceId: string; applianceId: string }>();
  const navigate = useNavigate();
  const { hooks } = useApi();

  // Mock appliance data (would come from API)
  const [appliance, setAppliance] = useState({
    id: applianceId ?? "unknown",
    name: "my-appliance",
    status: "running",
    templateId: "ubuntu-22.04",
    templateName: "Ubuntu 22.04",
    createdAt: new Date().toISOString(),
    memory: 4096,
    cpus: 4,
    diskSize: 50,
    ipAddress: "10.0.0.15",
    trustLevel: "attested" as const,
  });

  const [activeTab, setActiveTab] = useState("overview");
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Mock attachments
  const attachments: Attachment[] = [
    { id: "disk-0", type: "disk", name: "root-disk", details: "50 GB, virtio-blk" },
    { id: "disk-1", type: "disk", name: "data-disk", details: "100 GB, virtio-blk" },
    { id: "net-0", type: "network", name: "eth0", details: "Management (10.0.0.0/24)" },
    { id: "net-1", type: "network", name: "eth1", details: "Application (10.0.1.0/24)" },
  ];

  // Mock events
  const events: TimelineItem[] = [
    { id: "1", time: "2 min ago", title: "Appliance started", status: "success" },
    { id: "2", time: "5 min ago", title: "Network attached: eth1", status: "muted" },
    { id: "3", time: "1 hour ago", title: "Disk resized to 50 GB", status: "muted" },
    { id: "4", time: "1 day ago", title: "Appliance created", status: "success" },
  ];

  // Lifecycle actions
  async function handleStart() {
    setBusy(true);
    await new Promise((r) => setTimeout(r, 1000));
    setAppliance((a) => ({ ...a, status: "running" }));
    setBusy(false);
  }

  async function handleStop() {
    setBusy(true);
    await new Promise((r) => setTimeout(r, 1000));
    setAppliance((a) => ({ ...a, status: "stopped" }));
    setBusy(false);
  }

  async function handleReboot() {
    setBusy(true);
    setAppliance((a) => ({ ...a, status: "rebooting" }));
    await new Promise((r) => setTimeout(r, 2000));
    setAppliance((a) => ({ ...a, status: "running" }));
    setBusy(false);
  }

  async function handleDelete() {
    setBusy(true);
    await new Promise((r) => setTimeout(r, 1000));
    navigate(`/workspaces/${workspaceId}/appliances`);
  }

  // Property rows
  const properties: PropertyRow[] = [
    { label: "ID", value: appliance.id },
    { label: "Name", value: appliance.name },
    { label: "Status", value: <StatusChip status={appliance.status === "running" ? "success" : appliance.status === "stopped" ? "muted" : "warning"}>{appliance.status}</StatusChip> },
    { label: "Template", value: appliance.templateName },
    { label: "Memory", value: `${appliance.memory} MB` },
    { label: "CPUs", value: appliance.cpus },
    { label: "Disk", value: `${appliance.diskSize} GB` },
    { label: "IP Address", value: appliance.ipAddress ?? "—" },
    { label: "Trust", value: <TrustBadge level={appliance.trustLevel} /> },
    { label: "Created", value: new Date(appliance.createdAt).toLocaleString() },
  ];

  // Tabs
  const tabs: TabItem[] = [
    { id: "overview", label: "Overview" },
    { id: "console", label: "Console" },
    { id: "serial", label: "Serial" },
    { id: "attachments", label: "Attachments" },
    { id: "snapshots", label: "Snapshots" },
    { id: "events", label: "Events" },
  ];

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title={appliance.name}
        subtitle={`${appliance.templateName} • ${appliance.id}`}
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Appliances", href: `/workspaces/${workspaceId}/appliances` },
          { label: appliance.name },
        ]}
        actions={
          <LifecycleControls
            status={appliance.status}
            onStart={handleStart}
            onStop={handleStop}
            onReboot={handleReboot}
            onDelete={() => setConfirmDelete(true)}
            busy={busy}
          />
        }
      />

      <DockLayout
        left={
          <Panel title="Details">
            <PropertyGrid rows={properties} />
          </Panel>
        }
        center={
          <Card style={{ padding: 16 }}>
            <Tabs tabs={tabs} activeId={activeTab} onChange={setActiveTab} />

            <div style={{ marginTop: 16 }}>
              {activeTab === "overview" && (
                <div>
                  <h4>Quick Stats</h4>
                  <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 12, marginTop: 12 }}>
                    <Card style={{ padding: 12, textAlign: "center" }}>
                      <div style={{ fontSize: 24, fontWeight: 600 }}>{appliance.cpus}</div>
                      <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>CPUs</div>
                    </Card>
                    <Card style={{ padding: 12, textAlign: "center" }}>
                      <div style={{ fontSize: 24, fontWeight: 600 }}>{appliance.memory / 1024} GB</div>
                      <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>Memory</div>
                    </Card>
                    <Card style={{ padding: 12, textAlign: "center" }}>
                      <div style={{ fontSize: 24, fontWeight: 600 }}>{appliance.diskSize} GB</div>
                      <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>Storage</div>
                    </Card>
                    <Card style={{ padding: 12, textAlign: "center" }}>
                      <div style={{ fontSize: 24, fontWeight: 600 }}>{attachments.filter((a) => a.type === "network").length}</div>
                      <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>Networks</div>
                    </Card>
                  </div>
                </div>
              )}

              {activeTab === "console" && <ConsoleSurface applianceId={appliance.id} connected={appliance.status === "running"} />}

              {activeTab === "serial" && <SerialConsole applianceId={appliance.id} />}

              {activeTab === "attachments" && <AttachmentPanel attachments={attachments} />}

              {activeTab === "snapshots" && (
                <div>
                  <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 16 }}>
                    <h4>Snapshots</h4>
                    <Button variant="primary" size="sm">Create Snapshot</Button>
                  </div>
                  <p style={{ color: "var(--ifm-color-subtle)" }}>No snapshots yet</p>
                </div>
              )}

              {activeTab === "events" && <Timeline items={events} />}
            </div>
          </Card>
        }
        right={
          <Panel title="Quick Actions">
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <Button variant="secondary" size="sm">Create Snapshot</Button>
              <Button variant="secondary" size="sm">Clone Appliance</Button>
              <Button variant="secondary" size="sm">Export Config</Button>
              <Button variant="secondary" size="sm">View Logs</Button>
            </div>
          </Panel>
        }
      />

      <ConfirmDialog
        open={confirmDelete}
        title="Delete Appliance"
        description={`Are you sure you want to delete "${appliance.name}"? This action cannot be undone.`}
        confirmLabel="Delete"
        tone="danger"
        onConfirm={handleDelete}
        onCancel={() => setConfirmDelete(false)}
        loading={busy}
      />
    </div>
  );
}

export default ApplianceDetail;
