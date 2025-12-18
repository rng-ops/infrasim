import React, { useState, useRef, useEffect, useMemo, useCallback } from "react";
import {
  PageHeader,
  Button,
  Card,
  StatusChip,
  PropertyGrid,
  Panel,
  DockLayout,
  Tabs,
  SearchInput,
  Spinner,
  EmptyState,
  Toolbar,
  ToolbarSpacer,
  ToolbarDivider,
  DiffList,
  Stepper,
  Dialog,
  ConfirmDialog,
  FormField,
  Input,
  Select,
} from "@infrasim/ui";
import type { DiffItem, StepperItem } from "@infrasim/ui";
import { useApi } from "../api-context";
import { useStore, useActions } from "../store/store";

// Types inline to avoid build order issues
interface ResourceNode {
  id: string;
  resource_type: string;
  address: string;
  status: string;
  metadata: unknown;
  position?: { x: number; y: number };
}

interface ResourceEdge {
  id: string;
  source: string;
  target: string;
  edge_type: string;
  metadata: unknown;
}

interface GraphPlanResult {
  plan_id: string;
  additions: string[];
  modifications: string[];
  deletions: string[];
  valid: boolean;
  errors: string[];
}

interface Filesystem {
  id: string;
  name: string;
  fs_type: string;
  size_bytes: number;
  lifecycle: string;
  attached_to: string[];
  labels: Record<string, string>;
}

interface UiManifest {
  version: string;
  build_timestamp: string;
  git_commit?: string;
  git_branch?: string;
}

// ============================================================================
// Resource Graph Canvas (Interactive DAG Visualization)
// ============================================================================

interface GraphCanvasProps {
  nodes: ResourceNode[];
  edges: ResourceEdge[];
  selectedIds: string[];
  onSelect: (ids: string[]) => void;
  onNodeDrag?: (nodeId: string, position: { x: number; y: number }) => void;
}

const NODE_COLORS: Record<string, string> = {
  appliance: "#3b82f6",
  "filesystem.local": "#22c55e",
  "filesystem.snapshot": "#8b5cf6",
  "filesystem.ephemeral": "#f59e0b",
  "filesystem.network": "#06b6d4",
  "filesystem.physical": "#6366f1",
  "filesystem.geobound": "#ef4444",
  network: "#f97316",
  volume: "#84cc16",
  default: "#6b7280",
};

const STATUS_COLORS: Record<string, string> = {
  running: "#22c55e",
  stopped: "#6b7280",
  pending: "#f59e0b",
  ready: "#22c55e",
  attached: "#3b82f6",
  error: "#ef4444",
  creating: "#f59e0b",
  deleting: "#ef4444",
  default: "#6b7280",
};

function autoLayoutNodes(nodes: ResourceNode[]): ResourceNode[] {
  // Simple horizontal layout if nodes don't have positions
  const SPACING_X = 180;
  const SPACING_Y = 120;
  const MARGIN = 100;

  // Group by type
  const byType: Record<string, ResourceNode[]> = {};
  nodes.forEach((n) => {
    const type = n.resource_type.split(".")[0];
    if (!byType[type]) byType[type] = [];
    byType[type].push(n);
  });

  const result: ResourceNode[] = [];
  let row = 0;

  Object.entries(byType).forEach(([type, typeNodes]) => {
    typeNodes.forEach((node, col) => {
      result.push({
        ...node,
        position: node.position ?? { x: MARGIN + col * SPACING_X, y: MARGIN + row * SPACING_Y },
      });
    });
    row++;
  });

  return result;
}

function GraphCanvas({ nodes, edges, selectedIds, onSelect, onNodeDrag }: GraphCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [dragging, setDragging] = useState<{ id: string; offsetX: number; offsetY: number } | null>(null);
  const [localNodes, setLocalNodes] = useState<ResourceNode[]>([]);

  // Auto-layout on mount or when nodes change
  useEffect(() => {
    setLocalNodes(autoLayoutNodes(nodes));
  }, [nodes]);

  // Draw canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Scale for retina
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    // Clear
    ctx.fillStyle = "#0f172a";
    ctx.fillRect(0, 0, rect.width, rect.height);

    // Draw grid
    ctx.strokeStyle = "#1e293b";
    ctx.lineWidth = 1;
    for (let x = 0; x < rect.width; x += 40) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, rect.height);
      ctx.stroke();
    }
    for (let y = 0; y < rect.height; y += 40) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(rect.width, y);
      ctx.stroke();
    }

    // Draw edges
    edges.forEach((edge) => {
      const from = localNodes.find((n) => n.id === edge.source);
      const to = localNodes.find((n) => n.id === edge.target);
      if (!from?.position || !to?.position) return;

      const isConnectedToSelected = selectedIds.includes(edge.source) || selectedIds.includes(edge.target);

      ctx.strokeStyle = isConnectedToSelected ? "#60a5fa" : "#475569";
      ctx.lineWidth = isConnectedToSelected ? 2 : 1;

      ctx.beginPath();
      ctx.moveTo(from.position.x, from.position.y);

      // Bezier curve for nicer edges
      const midX = (from.position.x + to.position.x) / 2;
      ctx.bezierCurveTo(midX, from.position.y, midX, to.position.y, to.position.x, to.position.y);

      ctx.stroke();

      // Arrow at target
      const angle = Math.atan2(to.position.y - from.position.y, to.position.x - from.position.x);
      const arrowSize = 8;
      const arrowX = to.position.x - 30 * Math.cos(angle);
      const arrowY = to.position.y - 30 * Math.sin(angle);

      ctx.fillStyle = isConnectedToSelected ? "#60a5fa" : "#475569";
      ctx.beginPath();
      ctx.moveTo(arrowX, arrowY);
      ctx.lineTo(arrowX - arrowSize * Math.cos(angle - Math.PI / 6), arrowY - arrowSize * Math.sin(angle - Math.PI / 6));
      ctx.lineTo(arrowX - arrowSize * Math.cos(angle + Math.PI / 6), arrowY - arrowSize * Math.sin(angle + Math.PI / 6));
      ctx.closePath();
      ctx.fill();
    });

    // Draw nodes
    localNodes.forEach((node) => {
      if (!node.position) return;

      const isSelected = selectedIds.includes(node.id);
      const isHovered = node.id === hoveredId;

      const nodeColor = NODE_COLORS[node.resource_type] ?? NODE_COLORS.default;
      const statusColor = STATUS_COLORS[node.status] ?? STATUS_COLORS.default;

      // Node box
      const width = 140;
      const height = 60;
      const x = node.position.x - width / 2;
      const y = node.position.y - height / 2;

      // Shadow
      if (isSelected || isHovered) {
        ctx.shadowColor = nodeColor;
        ctx.shadowBlur = 20;
      }

      ctx.fillStyle = isSelected ? nodeColor : isHovered ? "#1e3a5f" : "#1e293b";
      ctx.strokeStyle = isSelected ? "#fff" : isHovered ? nodeColor : "#334155";
      ctx.lineWidth = isSelected ? 2 : 1;

      ctx.beginPath();
      ctx.roundRect(x, y, width, height, 8);
      ctx.fill();
      ctx.stroke();

      ctx.shadowColor = "transparent";
      ctx.shadowBlur = 0;

      // Type icon bar
      ctx.fillStyle = nodeColor;
      ctx.fillRect(x, y, 6, height);

      // Status indicator
      ctx.fillStyle = statusColor;
      ctx.beginPath();
      ctx.arc(x + width - 12, y + 12, 4, 0, Math.PI * 2);
      ctx.fill();

      // Labels
      ctx.fillStyle = "#f8fafc";
      ctx.font = "bold 11px system-ui, sans-serif";
      ctx.textAlign = "left";

      // Truncate address
      const address = node.address.length > 22 ? node.address.slice(0, 20) + "…" : node.address;
      ctx.fillText(address, x + 12, y + 20);

      ctx.fillStyle = "#94a3b8";
      ctx.font = "10px system-ui, sans-serif";
      ctx.fillText(node.resource_type, x + 12, y + 35);

      ctx.fillStyle = statusColor;
      ctx.fillText(node.status, x + 12, y + 50);
    });
  }, [localNodes, edges, selectedIds, hoveredId]);

  function getNodeAt(x: number, y: number): ResourceNode | undefined {
    return localNodes.find((n) => {
      if (!n.position) return false;
      const dx = Math.abs(x - n.position.x);
      const dy = Math.abs(y - n.position.y);
      return dx < 70 && dy < 30;
    });
  }

  function handleMouseDown(e: React.MouseEvent<HTMLCanvasElement>) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    const node = getNodeAt(x, y);
    if (node && node.position) {
      setDragging({
        id: node.id,
        offsetX: x - node.position.x,
        offsetY: y - node.position.y,
      });
    }
  }

  function handleMouseUp(e: React.MouseEvent<HTMLCanvasElement>) {
    if (dragging) {
      const canvas = canvasRef.current;
      if (canvas) {
        const rect = canvas.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const y = e.clientY - rect.top;
        onNodeDrag?.(dragging.id, { x: x - dragging.offsetX, y: y - dragging.offsetY });
      }
      setDragging(null);
    }
  }

  function handleMouseMove(e: React.MouseEvent<HTMLCanvasElement>) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    if (dragging) {
      setLocalNodes((prev) =>
        prev.map((n) =>
          n.id === dragging.id
            ? { ...n, position: { x: x - dragging.offsetX, y: y - dragging.offsetY } }
            : n
        )
      );
      canvas.style.cursor = "grabbing";
    } else {
      const hovered = getNodeAt(x, y);
      setHoveredId(hovered?.id ?? null);
      canvas.style.cursor = hovered ? "pointer" : "default";
    }
  }

  function handleClick(e: React.MouseEvent<HTMLCanvasElement>) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    const clicked = getNodeAt(x, y);
    if (clicked) {
      if (e.shiftKey || e.metaKey) {
        // Multi-select
        onSelect(
          selectedIds.includes(clicked.id)
            ? selectedIds.filter((id) => id !== clicked.id)
            : [...selectedIds, clicked.id]
        );
      } else {
        onSelect([clicked.id]);
      }
    } else {
      onSelect([]);
    }
  }

  return (
    <canvas
      ref={canvasRef}
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => {
        setHoveredId(null);
        if (dragging) setDragging(null);
      }}
      style={{ width: "100%", height: 500, borderRadius: 8, background: "#0f172a" }}
      aria-label="Resource graph diagram. Use keyboard to navigate."
      tabIndex={0}
      role="img"
    />
  );
}

// ============================================================================
// Plan Preview Dialog
// ============================================================================

interface PlanPreviewProps {
  open: boolean;
  plan: GraphPlanResult | null;
  onApply: () => void;
  onCancel: () => void;
  loading: boolean;
}

function PlanPreviewDialog({ open, plan, onApply, onCancel, loading }: PlanPreviewProps) {
  if (!plan) return null;

  const diffItems: DiffItem[] = [
    ...plan.additions.map((a) => ({ type: "add" as const, name: a, resourceType: "resource", changes: [] })),
    ...plan.modifications.map((m) => ({ type: "update" as const, name: m, resourceType: "resource", changes: [] })),
    ...plan.deletions.map((d) => ({ type: "delete" as const, name: d, resourceType: "resource", changes: [] })),
  ];

  return (
    <Dialog
      open={open}
      title="Apply Changes"
      description={`This will apply ${plan.additions.length} additions, ${plan.modifications.length} modifications, and ${plan.deletions.length} deletions.`}
      onClose={onCancel}
      footer={
        <>
          <Button variant="secondary" onClick={onCancel} disabled={loading}>
            Cancel
          </Button>
          <Button variant="primary" onClick={onApply} loading={loading} disabled={!plan.valid}>
            Apply
          </Button>
        </>
      }
    >
      {plan.errors.length > 0 && (
        <div style={{ marginBottom: 16, padding: 12, background: "rgba(239,68,68,0.1)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.3)" }}>
          <strong style={{ color: "#ef4444" }}>Validation Errors:</strong>
          <ul style={{ margin: "8px 0 0 16px", padding: 0 }}>
            {plan.errors.map((e, i) => (
              <li key={i} style={{ color: "#f87171" }}>{e}</li>
            ))}
          </ul>
        </div>
      )}
      <DiffList items={diffItems} />
    </Dialog>
  );
}

// ============================================================================
// Create Filesystem Dialog
// ============================================================================

interface CreateFilesystemDialogProps {
  open: boolean;
  onClose: () => void;
  onCreate: (data: { name: string; fs_type: string; size_bytes: number }) => void;
  loading: boolean;
}

function CreateFilesystemDialog({ open, onClose, onCreate, loading }: CreateFilesystemDialogProps) {
  const [name, setName] = useState("");
  const [fsType, setFsType] = useState("local");
  const [sizeMb, setSizeMb] = useState(1024);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    onCreate({ name, fs_type: fsType, size_bytes: sizeMb * 1024 * 1024 });
  }

  return (
    <Dialog
      open={open}
      title="Create Filesystem"
      description="Create a new Terraform-addressable virtual filesystem."
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={loading}>
            Cancel
          </Button>
          <Button variant="primary" onClick={handleSubmit as any} loading={loading} disabled={!name.trim()}>
            Create
          </Button>
        </>
      }
    >
      <form onSubmit={handleSubmit}>
        <FormField label="Name" required>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="my-filesystem"
            aria-describedby="fs-name-hint"
          />
        </FormField>

        <FormField label="Type" hint="Select filesystem type for Terraform addressing (e.g., infrasim_filesystem.local)">
          <Select value={fsType} onChange={(e) => setFsType(e.target.value)}>
            <option value="local">fs.local - Host-local storage</option>
            <option value="snapshot">fs.snapshot - Copy-on-write snapshot</option>
            <option value="ephemeral">fs.ephemeral - RAM-backed (lost on stop)</option>
            <option value="network">fs.network - NFS/CIFS/iSCSI mount</option>
            <option value="physical">fs.physical - Direct block device</option>
            <option value="geobound">fs.geobound - Geo-fenced with destruction policy</option>
          </Select>
        </FormField>

        <FormField label="Size (MB)">
          <Input
            type="number"
            value={sizeMb}
            onChange={(e) => setSizeMb(parseInt(e.target.value) || 1024)}
            min={1}
          />
        </FormField>
      </form>
    </Dialog>
  );
}

// ============================================================================
// Main Resource Graph Page
// ============================================================================

export function ResourceGraphPage() {
  const { hooks } = useApi();
  const { state } = useStore();
  const actions = useActions();

  // API queries
  const { data: graph, isLoading: graphLoading, refetch: refetchGraph } = hooks.useResourceGraph();
  const { data: validation } = hooks.useValidateGraph();
  const { data: filesystems, isLoading: filesystemsLoading } = hooks.useFilesystems();
  const { data: manifest } = hooks.useUiManifest();

  const planMutation = hooks.usePlanGraphChanges();
  const applyMutation = hooks.useApplyGraphChanges();
  const createFsMutation = hooks.useCreateFilesystem();

  // Local state
  const [activeTab, setActiveTab] = useState("graph");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const [showPlanDialog, setShowPlanDialog] = useState(false);
  const [showCreateFsDialog, setShowCreateFsDialog] = useState(false);
  const [pendingPlan, setPendingPlan] = useState<GraphPlanResult | null>(null);

  // Nodes and edges from API
  const nodes = useMemo<ResourceNode[]>(() => (graph?.nodes as ResourceNode[]) ?? [], [graph]);
  const edges = useMemo<ResourceEdge[]>(() => (graph?.edges as ResourceEdge[]) ?? [], [graph]);

  // Filtered nodes for list view
  const filteredNodes = useMemo<ResourceNode[]>(() => {
    if (!search) return nodes;
    const q = search.toLowerCase();
    return nodes.filter(
      (n: ResourceNode) =>
        n.address.toLowerCase().includes(q) ||
        n.resource_type.toLowerCase().includes(q) ||
        n.status.toLowerCase().includes(q)
    );
  }, [nodes, search]);

  // Selected node details
  const selectedNode = selectedIds.length === 1 ? nodes.find((n: ResourceNode) => n.id === selectedIds[0]) : null;

  // Handle plan generation
  async function handlePlan() {
    const result = await planMutation.mutateAsync({ operations: [] });
    setPendingPlan(result);
    setShowPlanDialog(true);
  }

  // Handle apply
  async function handleApply() {
    if (!pendingPlan) return;
    await applyMutation.mutateAsync({ plan_id: pendingPlan.plan_id });
    setShowPlanDialog(false);
    setPendingPlan(null);
    refetchGraph();
    actions.pushToast({ title: "Graph changes applied", tone: "success" });
  }

  // Handle filesystem creation
  async function handleCreateFilesystem(data: { name: string; fs_type: string; size_bytes: number }) {
    await createFsMutation.mutateAsync({
      name: data.name,
      fs_type: data.fs_type as any,
      size_bytes: data.size_bytes,
      lifecycle: "pending",
      attached_to: [],
      labels: {},
    });
    setShowCreateFsDialog(false);
    refetchGraph();
    actions.pushToast({ title: `Filesystem "${data.name}" created`, tone: "success" });
  }

  // Tabs
  const tabs = [
    { id: "graph", label: "Graph View" },
    { id: "list", label: "Resources" },
    { id: "filesystems", label: "Filesystems" },
  ];

  if (graphLoading) {
    return (
      <div style={{ padding: 24, display: "flex", justifyContent: "center", alignItems: "center", height: 400 }}>
        <Spinner label="Loading resource graph..." />
      </div>
    );
  }

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Resource Graph"
        description="Terraform-addressable resource topology with virtual filesystems"
        actions={
          <div style={{ display: "flex", gap: 8 }}>
            <Button variant="secondary" onClick={() => setShowCreateFsDialog(true)}>
              Add Filesystem
            </Button>
            <Button variant="primary" onClick={handlePlan} disabled={planMutation.isPending}>
              Plan Changes
            </Button>
          </div>
        }
      />

      {/* Validation warnings */}
      {validation && !validation.valid && (
        <div style={{ marginBottom: 16 }}>
          <Card padding="sm">
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <StatusChip label="Validation Issues" tone="danger" />
            <span style={{ color: "var(--ifm-color-subtle)" }}>
              {validation.errors.length} errors, {validation.warnings.length} warnings
            </span>
          </div>
        </Card>
        </div>
      )}

      <DockLayout
        center={
          <Card padding="md">
            <Toolbar>
              <Tabs
                items={tabs.map((t) => ({ id: t.id, label: t.label, panel: null }))}
                initialId={activeTab}
              />
              <ToolbarSpacer />
              <SearchInput value={search} onChange={setSearch} placeholder="Filter resources..." />
              <ToolbarDivider />
              <Button variant="ghost" size="sm" onClick={() => refetchGraph()} aria-label="Refresh graph">
                ↻ Refresh
              </Button>
            </Toolbar>

            <div style={{ marginTop: 16 }}>
              {activeTab === "graph" && (
                <>
                  {nodes.length === 0 ? (
                    <EmptyState
                      title="No resources"
                      description="Create appliances or filesystems to see them in the resource graph."
                      actions={
                        <Button variant="primary" onClick={() => setShowCreateFsDialog(true)}>
                          Create Filesystem
                        </Button>
                      }
                    />
                  ) : (
                    <GraphCanvas
                      nodes={nodes}
                      edges={edges}
                      selectedIds={selectedIds}
                      onSelect={setSelectedIds}
                    />
                  )}
                </>
              )}

              {activeTab === "list" && (
                <>
                  {filteredNodes.length === 0 ? (
                    <EmptyState title="No matching resources" description="Try adjusting your search filter." />
                  ) : (
                    <table className="ifm-table" aria-label="Resource list">
                      <thead>
                        <tr>
                          <th>Address</th>
                          <th>Type</th>
                          <th>Status</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {filteredNodes.map((node) => (
                          <tr
                            key={node.id}
                            onClick={() => setSelectedIds([node.id])}
                            style={{ cursor: "pointer", background: selectedIds.includes(node.id) ? "rgba(59,130,246,0.1)" : undefined }}
                          >
                            <td><code>{node.address}</code></td>
                            <td><StatusChip label={node.resource_type} tone="info" size="sm" /></td>
                            <td>
                              <StatusChip
                                label={node.status}
                                tone={node.status === "running" || node.status === "ready" ? "success" : node.status === "error" ? "danger" : "muted"}
                                size="sm"
                              />
                            </td>
                            <td>
                              <Button variant="ghost" size="sm" onClick={() => setSelectedIds([node.id])}>
                                Details
                              </Button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </>
              )}

              {activeTab === "filesystems" && (
                <>
                  {filesystemsLoading ? (
                    <Spinner label="Loading filesystems..." />
                  ) : !filesystems || filesystems.length === 0 ? (
                    <EmptyState
                      title="No filesystems"
                      description="Virtual filesystems provide Terraform-addressable storage for appliances."
                      actions={
                        <Button variant="primary" onClick={() => setShowCreateFsDialog(true)}>
                          Create Filesystem
                        </Button>
                      }
                    />
                  ) : (
                    <table className="ifm-table" aria-label="Filesystems">
                      <thead>
                        <tr>
                          <th>Name</th>
                          <th>Type</th>
                          <th>Size</th>
                          <th>Status</th>
                          <th>Attached To</th>
                        </tr>
                      </thead>
                      <tbody>
                        {(filesystems as Filesystem[]).map((fs: Filesystem) => (
                          <tr key={fs.id}>
                            <td><code>infrasim_filesystem.{fs.name}</code></td>
                            <td><StatusChip label={`fs.${fs.fs_type}`} tone="info" size="sm" /></td>
                            <td>{(fs.size_bytes / 1024 / 1024).toFixed(0)} MB</td>
                            <td>
                              <StatusChip
                                label={fs.lifecycle}
                                tone={fs.lifecycle === "ready" || fs.lifecycle === "attached" ? "success" : fs.lifecycle === "error" ? "danger" : "muted"}
                                size="sm"
                              />
                            </td>
                            <td>
                              {fs.attached_to.length > 0 ? (
                                fs.attached_to.map((id: string) => (
                                  <StatusChip key={id} label={id.slice(0, 8)} tone="muted" size="sm" />
                                ))
                              ) : (
                                <span style={{ color: "var(--ifm-color-subtle)" }}>—</span>
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </>
              )}
            </div>
          </Card>
        }
        right={
          selectedNode && (
            <Panel title="Resource Details" actions={<Button variant="ghost" size="sm" onClick={() => setSelectedIds([])}>×</Button>}>
              <PropertyGrid
                rows={[
                  { label: "ID", value: <code style={{ fontSize: 11 }}>{selectedNode.id}</code> },
                  { label: "Address", value: <code>{selectedNode.address}</code> },
                  { label: "Type", value: <StatusChip label={selectedNode.resource_type} tone="info" size="sm" /> },
                  {
                    label: "Status",
                    value: (
                      <StatusChip
                        label={selectedNode.status}
                        tone={selectedNode.status === "running" || selectedNode.status === "ready" ? "success" : selectedNode.status === "error" ? "danger" : "muted"}
                        size="sm"
                      />
                    ),
                  },
                ]}
              />

              {selectedNode.metadata != null && typeof selectedNode.metadata === "object" && (
                <div style={{ marginTop: 16 }}>
                  <h5 style={{ margin: "0 0 8px", fontSize: 12, color: "var(--ifm-color-subtle)", textTransform: "uppercase" }}>Metadata</h5>
                  <pre style={{ fontSize: 11, background: "#0f172a", padding: 8, borderRadius: 4, overflow: "auto" }}>
                    {JSON.stringify(selectedNode.metadata as object, null, 2)}
                  </pre>
                </div>
              )}

              {/* Connected edges */}
              {edges.filter((e: ResourceEdge) => e.source === selectedNode.id || e.target === selectedNode.id).length > 0 && (
                <div style={{ marginTop: 16 }}>
                  <h5 style={{ margin: "0 0 8px", fontSize: 12, color: "var(--ifm-color-subtle)", textTransform: "uppercase" }}>Connections</h5>
                  <ul style={{ margin: 0, paddingLeft: 16 }}>
                    {edges
                      .filter((e: ResourceEdge) => e.source === selectedNode.id || e.target === selectedNode.id)
                      .map((edge: ResourceEdge) => {
                        const otherId = edge.source === selectedNode.id ? edge.target : edge.source;
                        const otherNode = nodes.find((n: ResourceNode) => n.id === otherId);
                        return (
                          <li key={edge.id} style={{ marginBottom: 4 }}>
                            <button
                              type="button"
                              style={{ background: "none", border: "none", color: "var(--ifm-color-accent)", cursor: "pointer", padding: 0 }}
                              onClick={() => setSelectedIds([otherId])}
                            >
                              {otherNode?.address ?? otherId}
                            </button>
                            <span style={{ color: "var(--ifm-color-subtle)", fontSize: 11, marginLeft: 6 }}>({edge.edge_type})</span>
                          </li>
                        );
                      })}
                  </ul>
                </div>
              )}
            </Panel>
          )
        }
        bottom={
          (manifest as UiManifest | undefined) && (
            <div style={{ padding: "8px 16px", fontSize: 11, color: "var(--ifm-color-subtle)", display: "flex", gap: 16 }}>
              <span>UI v{(manifest as UiManifest).version}</span>
              {(manifest as UiManifest).git_commit && <span>Commit: {(manifest as UiManifest).git_commit!.slice(0, 7)}</span>}
              <span>Built: {new Date((manifest as UiManifest).build_timestamp).toLocaleString()}</span>
            </div>
          )
        }
      />

      {/* Plan Preview Dialog */}
      <PlanPreviewDialog
        open={showPlanDialog}
        plan={pendingPlan}
        onApply={handleApply}
        onCancel={() => {
          setShowPlanDialog(false);
          setPendingPlan(null);
        }}
        loading={applyMutation.isPending}
      />

      {/* Create Filesystem Dialog */}
      <CreateFilesystemDialog
        open={showCreateFsDialog}
        onClose={() => setShowCreateFsDialog(false)}
        onCreate={handleCreateFilesystem}
        loading={createFsMutation.isPending}
      />
    </div>
  );
}

export default ResourceGraphPage;
