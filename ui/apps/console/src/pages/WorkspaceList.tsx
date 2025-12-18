import React, { useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Table,
  StatusChip,
  SearchInput,
  Dialog,
  FormField,
  Input,
  EmptyState,
  Spinner,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Workspace Types
// ============================================================================

interface Workspace {
  id: string;
  name: string;
  description: string;
  applianceCount: number;
  networkCount: number;
  status: "active" | "archived";
  createdAt: string;
  owner: string;
}

// ============================================================================
// Create Workspace Dialog
// ============================================================================

interface CreateWorkspaceDialogProps {
  open: boolean;
  onClose: () => void;
  onCreate: (name: string, description: string) => void;
}

function CreateWorkspaceDialog({ open, onClose, onCreate }: CreateWorkspaceDialogProps) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [creating, setCreating] = useState(false);

  async function handleCreate() {
    setCreating(true);
    await new Promise((r) => setTimeout(r, 1000));
    onCreate(name, description);
    setName("");
    setDescription("");
    setCreating(false);
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Create Workspace"
      description="Workspaces isolate appliances, networks, and resources"
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={creating}>Cancel</Button>
          <Button variant="primary" onClick={handleCreate} loading={creating} disabled={!name.trim()}>
            Create Workspace
          </Button>
        </>
      }
    >
      <FormField label="Workspace Name" required hint="A unique identifier for this workspace">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-workspace" />
      </FormField>

      <FormField label="Description">
        <Input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Optional description..." />
      </FormField>
    </Dialog>
  );
}

// ============================================================================
// Workspace Card
// ============================================================================

function WorkspaceCard({ workspace, onClick }: { workspace: Workspace; onClick: () => void }) {
  return (
    <Card
      style={{ cursor: "pointer", transition: "transform 0.15s, box-shadow 0.15s" }}
      onClick={onClick}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
        <h3 style={{ margin: 0, fontSize: 18 }}>{workspace.name}</h3>
        <StatusChip status={workspace.status === "active" ? "success" : "muted"}>{workspace.status}</StatusChip>
      </div>

      {workspace.description && (
        <p style={{ margin: "8px 0", color: "var(--ifm-color-subtle)", fontSize: 14 }}>{workspace.description}</p>
      )}

      <div style={{ display: "flex", gap: 16, marginTop: 12, fontSize: 13, color: "var(--ifm-color-muted)" }}>
        <div>
          <strong>{workspace.applianceCount}</strong> appliances
        </div>
        <div>
          <strong>{workspace.networkCount}</strong> networks
        </div>
      </div>

      <div style={{ marginTop: 12, fontSize: 12, color: "var(--ifm-color-muted)" }}>
        Created by {workspace.owner} • {new Date(workspace.createdAt).toLocaleDateString()}
      </div>
    </Card>
  );
}

// ============================================================================
// Main WorkspaceList Component
// ============================================================================

export function WorkspaceList() {
  const navigate = useNavigate();
  const { hooks } = useApi();

  const [search, setSearch] = useState("");
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [viewMode, setViewMode] = useState<"cards" | "table">("cards");

  // Mock workspaces
  const workspaces: Workspace[] = [
    {
      id: "ws-prod",
      name: "production",
      description: "Production environment",
      applianceCount: 12,
      networkCount: 4,
      status: "active",
      createdAt: new Date(Date.now() - 86400000 * 30).toISOString(),
      owner: "admin",
    },
    {
      id: "ws-staging",
      name: "staging",
      description: "Staging and QA environment",
      applianceCount: 8,
      networkCount: 3,
      status: "active",
      createdAt: new Date(Date.now() - 86400000 * 15).toISOString(),
      owner: "admin",
    },
    {
      id: "ws-dev",
      name: "development",
      description: "Development and testing",
      applianceCount: 24,
      networkCount: 6,
      status: "active",
      createdAt: new Date(Date.now() - 86400000 * 7).toISOString(),
      owner: "dev-team",
    },
    {
      id: "ws-sandbox",
      name: "sandbox",
      description: "Experimental sandbox environment",
      applianceCount: 3,
      networkCount: 1,
      status: "active",
      createdAt: new Date(Date.now() - 86400000 * 2).toISOString(),
      owner: "user1",
    },
    {
      id: "ws-archived",
      name: "old-project",
      description: "Archived project",
      applianceCount: 0,
      networkCount: 0,
      status: "archived",
      createdAt: new Date(Date.now() - 86400000 * 90).toISOString(),
      owner: "admin",
    },
  ];

  // Filter workspaces
  const filtered = search
    ? workspaces.filter(
        (ws) =>
          ws.name.toLowerCase().includes(search.toLowerCase()) ||
          ws.description.toLowerCase().includes(search.toLowerCase())
      )
    : workspaces;

  // Table columns
  const columns = [
    { key: "name", header: "Name", render: (ws: Workspace) => <a href="#" onClick={(e) => { e.preventDefault(); navigate(`/workspaces/${ws.id}/appliances`); }}>{ws.name}</a> },
    { key: "description", header: "Description", render: (ws: Workspace) => ws.description || "—" },
    { key: "appliances", header: "Appliances", render: (ws: Workspace) => ws.applianceCount },
    { key: "networks", header: "Networks", render: (ws: Workspace) => ws.networkCount },
    { key: "status", header: "Status", render: (ws: Workspace) => <StatusChip status={ws.status === "active" ? "success" : "muted"}>{ws.status}</StatusChip> },
    { key: "owner", header: "Owner", render: (ws: Workspace) => ws.owner },
    { key: "created", header: "Created", render: (ws: Workspace) => new Date(ws.createdAt).toLocaleDateString() },
  ];

  function handleCreate(name: string, description: string) {
    // Would call API here
    console.log("Creating workspace:", name, description);
  }

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Workspaces"
        subtitle="Manage your infrastructure workspaces"
        breadcrumbs={[{ label: "Home", href: "/" }, { label: "Workspaces" }]}
        actions={
          <Button variant="primary" onClick={() => setCreateDialogOpen(true)}>
            Create Workspace
          </Button>
        }
      />

      {/* Toolbar */}
      <div style={{ display: "flex", gap: 12, marginBottom: 16, alignItems: "center" }}>
        <SearchInput value={search} onChange={setSearch} placeholder="Search workspaces..." />

        <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
          <Button variant={viewMode === "cards" ? "primary" : "secondary"} size="sm" onClick={() => setViewMode("cards")}>
            Cards
          </Button>
          <Button variant={viewMode === "table" ? "primary" : "secondary"} size="sm" onClick={() => setViewMode("table")}>
            Table
          </Button>
        </div>
      </div>

      {/* Content */}
      {filtered.length === 0 ? (
        <EmptyState
          title="No workspaces found"
          description={search ? "Try adjusting your search" : "Create your first workspace to get started"}
          action={
            <Button variant="primary" onClick={() => setCreateDialogOpen(true)}>
              Create Workspace
            </Button>
          }
        />
      ) : viewMode === "cards" ? (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))", gap: 16 }}>
          {filtered.map((ws) => (
            <WorkspaceCard
              key={ws.id}
              workspace={ws}
              onClick={() => navigate(`/workspaces/${ws.id}/appliances`)}
            />
          ))}
        </div>
      ) : (
        <Card style={{ padding: 16 }}>
          <Table columns={columns} data={filtered} getRowKey={(ws) => ws.id} />
        </Card>
      )}

      <CreateWorkspaceDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        onCreate={handleCreate}
      />
    </div>
  );
}

export default WorkspaceList;
