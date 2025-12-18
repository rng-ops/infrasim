import React, { useState, useRef, useEffect, useMemo } from "react";
import { useParams, Link } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Table,
  StatusChip,
  PropertyGrid,
  PropertyRow,
  Panel,
  DockLayout,
  Tabs,
  TabItem,
  SearchInput,
  Spinner,
  EmptyState,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Network Topology Canvas (simplified)
// ============================================================================

interface TopologyNode {
  id: string;
  type: "network" | "appliance" | "router";
  label: string;
  x: number;
  y: number;
}

interface TopologyEdge {
  id: string;
  from: string;
  to: string;
}

interface TopologyCanvasProps {
  nodes: TopologyNode[];
  edges: TopologyEdge[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}

function TopologyCanvas({ nodes, edges, selectedId, onSelect }: TopologyCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Clear
    ctx.fillStyle = "#1a1a2e";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Draw edges
    ctx.strokeStyle = "#444";
    ctx.lineWidth = 2;
    edges.forEach((edge) => {
      const from = nodes.find((n) => n.id === edge.from);
      const to = nodes.find((n) => n.id === edge.to);
      if (from && to) {
        ctx.beginPath();
        ctx.moveTo(from.x, from.y);
        ctx.lineTo(to.x, to.y);
        ctx.stroke();
      }
    });

    // Draw nodes
    nodes.forEach((node) => {
      const isSelected = node.id === selectedId;
      const isHovered = node.id === hoveredId;

      // Node shape based on type
      ctx.fillStyle = isSelected ? "#007acc" : isHovered ? "#555" : node.type === "network" ? "#2e7d32" : node.type === "router" ? "#f57c00" : "#333";
      ctx.strokeStyle = isSelected ? "#00bcd4" : "#666";
      ctx.lineWidth = isSelected ? 3 : 1;

      if (node.type === "network") {
        // Rectangle for networks
        ctx.beginPath();
        ctx.roundRect(node.x - 40, node.y - 20, 80, 40, 8);
        ctx.fill();
        ctx.stroke();
      } else {
        // Circle for appliances
        ctx.beginPath();
        ctx.arc(node.x, node.y, 25, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }

      // Label
      ctx.fillStyle = "#fff";
      ctx.font = "12px sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(node.label, node.x, node.y + 4);
    });
  }, [nodes, edges, selectedId, hoveredId]);

  function handleClick(e: React.MouseEvent<HTMLCanvasElement>) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    // Find clicked node
    const clicked = nodes.find((n) => {
      const dx = x - n.x;
      const dy = y - n.y;
      return Math.sqrt(dx * dx + dy * dy) < 30;
    });

    onSelect(clicked?.id ?? null);
  }

  function handleMouseMove(e: React.MouseEvent<HTMLCanvasElement>) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    const hovered = nodes.find((n) => {
      const dx = x - n.x;
      const dy = y - n.y;
      return Math.sqrt(dx * dx + dy * dy) < 30;
    });

    setHoveredId(hovered?.id ?? null);
    canvas.style.cursor = hovered ? "pointer" : "default";
  }

  return (
    <canvas
      ref={canvasRef}
      width={800}
      height={500}
      onClick={handleClick}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoveredId(null)}
      style={{ width: "100%", height: "auto", borderRadius: 8, background: "#1a1a2e" }}
      aria-label="Network topology diagram"
    />
  );
}

// ============================================================================
// Network List
// ============================================================================

interface Network {
  id: string;
  name: string;
  cidr: string;
  type: "bridge" | "nat" | "isolated";
  applianceCount: number;
  status: "active" | "inactive";
}

// ============================================================================
// Main Network View
// ============================================================================

export function NetworkView() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const { hooks } = useApi();

  const [activeTab, setActiveTab] = useState("topology");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  // Mock networks
  const networks: Network[] = [
    { id: "net-mgmt", name: "Management", cidr: "10.0.0.0/24", type: "bridge", applianceCount: 5, status: "active" },
    { id: "net-app", name: "Application", cidr: "10.0.1.0/24", type: "nat", applianceCount: 12, status: "active" },
    { id: "net-dmz", name: "DMZ", cidr: "10.0.2.0/24", type: "isolated", applianceCount: 3, status: "active" },
    { id: "net-backup", name: "Backup", cidr: "10.0.10.0/24", type: "isolated", applianceCount: 2, status: "inactive" },
  ];

  // Mock topology
  const topologyNodes: TopologyNode[] = [
    { id: "net-mgmt", type: "network", label: "Management", x: 400, y: 80 },
    { id: "net-app", type: "network", label: "Application", x: 200, y: 250 },
    { id: "net-dmz", type: "network", label: "DMZ", x: 600, y: 250 },
    { id: "router-1", type: "router", label: "Router", x: 400, y: 180 },
    { id: "app-1", type: "appliance", label: "Web-1", x: 100, y: 350 },
    { id: "app-2", type: "appliance", label: "Web-2", x: 200, y: 380 },
    { id: "app-3", type: "appliance", label: "DB-1", x: 300, y: 350 },
    { id: "app-4", type: "appliance", label: "FW-1", x: 600, y: 350 },
    { id: "app-5", type: "appliance", label: "Jump", x: 700, y: 380 },
  ];

  const topologyEdges: TopologyEdge[] = [
    { id: "e1", from: "net-mgmt", to: "router-1" },
    { id: "e2", from: "router-1", to: "net-app" },
    { id: "e3", from: "router-1", to: "net-dmz" },
    { id: "e4", from: "net-app", to: "app-1" },
    { id: "e5", from: "net-app", to: "app-2" },
    { id: "e6", from: "net-app", to: "app-3" },
    { id: "e7", from: "net-dmz", to: "app-4" },
    { id: "e8", from: "net-dmz", to: "app-5" },
  ];

  // Filter networks
  const filteredNetworks = useMemo(() => {
    if (!search) return networks;
    const q = search.toLowerCase();
    return networks.filter((n) => n.name.toLowerCase().includes(q) || n.cidr.includes(q));
  }, [networks, search]);

  // Selected item details
  const selectedNetwork = networks.find((n) => n.id === selectedId);
  const selectedNode = topologyNodes.find((n) => n.id === selectedId);

  // Tabs
  const tabs: TabItem[] = [
    { id: "topology", label: "Topology" },
    { id: "list", label: "Networks" },
    { id: "subnets", label: "Subnets" },
  ];

  // Table columns
  const columns = [
    { key: "name", header: "Name", render: (n: Network) => <Link to={`/workspaces/${workspaceId}/networks/${n.id}`}>{n.name}</Link> },
    { key: "cidr", header: "CIDR", render: (n: Network) => n.cidr },
    { key: "type", header: "Type", render: (n: Network) => <StatusChip status="muted">{n.type}</StatusChip> },
    { key: "appliances", header: "Appliances", render: (n: Network) => n.applianceCount },
    { key: "status", header: "Status", render: (n: Network) => <StatusChip status={n.status === "active" ? "success" : "muted"}>{n.status}</StatusChip> },
  ];

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Network View"
        subtitle={workspaceId ? `Workspace: ${workspaceId}` : undefined}
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Networks" },
        ]}
        actions={
          <Button variant="primary" onClick={() => {}}>
            Create Network
          </Button>
        }
      />

      <DockLayout
        center={
          <Card style={{ padding: 16 }}>
            <Tabs tabs={tabs} activeId={activeTab} onChange={setActiveTab} />

            <div style={{ marginTop: 16 }}>
              {activeTab === "topology" && (
                <TopologyCanvas nodes={topologyNodes} edges={topologyEdges} selectedId={selectedId} onSelect={setSelectedId} />
              )}

              {activeTab === "list" && (
                <>
                  <div style={{ marginBottom: 16 }}>
                    <SearchInput value={search} onChange={setSearch} placeholder="Search networks..." />
                  </div>
                  <Table columns={columns} data={filteredNetworks} getRowKey={(n) => n.id} />
                </>
              )}

              {activeTab === "subnets" && (
                <EmptyState title="Subnet management" description="Configure subnet allocations and DHCP settings" />
              )}
            </div>
          </Card>
        }
        right={
          selectedId && (
            <Panel title="Details" actions={<Button variant="ghost" size="sm" onClick={() => setSelectedId(null)}>×</Button>}>
              {selectedNetwork ? (
                <PropertyGrid
                  rows={[
                    { label: "Name", value: selectedNetwork.name },
                    { label: "CIDR", value: selectedNetwork.cidr },
                    { label: "Type", value: selectedNetwork.type },
                    { label: "Appliances", value: selectedNetwork.applianceCount },
                    { label: "Status", value: <StatusChip status={selectedNetwork.status === "active" ? "success" : "muted"}>{selectedNetwork.status}</StatusChip> },
                  ]}
                />
              ) : selectedNode ? (
                <PropertyGrid
                  rows={[
                    { label: "ID", value: selectedNode.id },
                    { label: "Type", value: selectedNode.type },
                    { label: "Label", value: selectedNode.label },
                  ]}
                />
              ) : (
                <p style={{ color: "var(--ifm-color-subtle)" }}>Select an item to view details</p>
              )}

              {selectedNetwork && (
                <div style={{ marginTop: 16 }}>
                  <h5>Connected Appliances</h5>
                  <ul style={{ margin: 0, paddingLeft: 16 }}>
                    <li><Link to="#">web-server-1</Link></li>
                    <li><Link to="#">db-primary</Link></li>
                    <li><Link to="#">cache-node</Link></li>
                  </ul>
                </div>
              )}
            </Panel>
          )
        }
      />
    </div>
  );
}

export default NetworkView;
