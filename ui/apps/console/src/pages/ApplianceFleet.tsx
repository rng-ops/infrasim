import React, { useState, useMemo } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Table,
  StatusChip,
  FilterBar,
  FilterChip,
  SearchInput,
  BulkActionBar,
  EmptyState,
  Spinner,
  Tabs,
  ConfirmDialog,
  CapabilityGate,
} from "@infrasim/ui";
import { useStore } from "../store/store";
import { useApi } from "../api-context";

// ============================================================================
// Appliance Card Grid (card view)
// ============================================================================

interface ApplianceCardProps {
  id: string;
  name: string;
  status: string;
  template: string;
  created: string;
  selected: boolean;
  onSelect: (id: string) => void;
  onClick: () => void;
}

function ApplianceCard({ id, name, status, template, created, selected, onSelect, onClick }: ApplianceCardProps) {
  const statusVariant = status === "running" ? "success" : status === "stopped" ? "muted" : status === "error" ? "danger" : "warning";
  return (
    <Card
      style={{
        cursor: "pointer",
        border: selected ? "2px solid var(--ifm-color-primary)" : undefined,
        opacity: 1,
      }}
      onClick={onClick}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 8 }}>
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onSelect(id)}
          onClick={(e) => e.stopPropagation()}
          aria-label={`Select ${name}`}
        />
        <StatusChip status={statusVariant}>{status}</StatusChip>
      </div>
      <h3 style={{ margin: "0 0 8px 0", fontSize: 16 }}>{name}</h3>
      <div style={{ fontSize: 12, color: "var(--ifm-color-subtle)" }}>
        <div>Template: {template}</div>
        <div>Created: {created}</div>
      </div>
    </Card>
  );
}

// ============================================================================
// Appliance Table Row
// ============================================================================

interface ApplianceRowData {
  id: string;
  name: string;
  status: string;
  template: string;
  networks: string[];
  created: string;
}

// ============================================================================
// Fleet Page Component
// ============================================================================

type ViewMode = "table" | "cards";
type StatusFilter = "all" | "running" | "stopped" | "error";

export function ApplianceFleet() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const navigate = useNavigate();
  const { hooks } = useApi();
  const { state, dispatch } = useStore();

  // View mode
  const [viewMode, setViewMode] = useState<ViewMode>("table");

  // Filters
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [templateFilter, setTemplateFilter] = useState<string | null>(null);

  // Selection
  const [selection, setSelection] = useState<Set<string>>(new Set());

  // Bulk action dialogs
  const [confirmBulkStop, setConfirmBulkStop] = useState(false);
  const [confirmBulkDelete, setConfirmBulkDelete] = useState(false);
  const [bulkBusy, setBulkBusy] = useState(false);

  // Fetch appliances
  const appliancesQuery = hooks.useWorkspaceAppliances(workspaceId ?? "");
  const appliances = appliancesQuery.data ?? [];

  // Derive template options for filter
  const templates = useMemo(() => {
    const set = new Set<string>();
    appliances.forEach((a) => set.add(a.templateId));
    return Array.from(set);
  }, [appliances]);

  // Filter appliances
  const filtered = useMemo(() => {
    let list = appliances;
    if (search) {
      const q = search.toLowerCase();
      list = list.filter((a) => a.name.toLowerCase().includes(q) || a.id.toLowerCase().includes(q));
    }
    if (statusFilter !== "all") {
      list = list.filter((a) => a.status === statusFilter);
    }
    if (templateFilter) {
      list = list.filter((a) => a.templateId === templateFilter);
    }
    return list;
  }, [appliances, search, statusFilter, templateFilter]);

  // Selection helpers
  const toggleSelect = (id: string) => {
    setSelection((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const selectAll = () => setSelection(new Set(filtered.map((a) => a.id)));
  const clearSelection = () => setSelection(new Set());

  // Bulk actions
  async function handleBulkStop() {
    setBulkBusy(true);
    // TODO: call API to stop each selected appliance
    await new Promise((r) => setTimeout(r, 1000));
    setBulkBusy(false);
    setConfirmBulkStop(false);
    clearSelection();
    appliancesQuery.refetch();
  }

  async function handleBulkDelete() {
    setBulkBusy(true);
    // TODO: call API to delete each selected appliance
    await new Promise((r) => setTimeout(r, 1000));
    setBulkBusy(false);
    setConfirmBulkDelete(false);
    clearSelection();
    appliancesQuery.refetch();
  }

  // Row data for table
  const rows: ApplianceRowData[] = filtered.map((a) => ({
    id: a.id,
    name: a.name,
    status: a.status,
    template: a.templateId,
    networks: a.networks ?? [],
    created: new Date(a.createdAt).toLocaleDateString(),
  }));

  // Table columns
  const columns = [
    {
      key: "select",
      header: (
        <input
          type="checkbox"
          checked={selection.size === filtered.length && filtered.length > 0}
          onChange={(e) => (e.target.checked ? selectAll() : clearSelection())}
          aria-label="Select all"
        />
      ),
      render: (row: ApplianceRowData) => (
        <input
          type="checkbox"
          checked={selection.has(row.id)}
          onChange={() => toggleSelect(row.id)}
          aria-label={`Select ${row.name}`}
        />
      ),
    },
    { key: "name", header: "Name", render: (row: ApplianceRowData) => <Link to={`/workspaces/${workspaceId}/appliances/${row.id}`}>{row.name}</Link> },
    {
      key: "status",
      header: "Status",
      render: (row: ApplianceRowData) => {
        const variant = row.status === "running" ? "success" : row.status === "stopped" ? "muted" : row.status === "error" ? "danger" : "warning";
        return <StatusChip status={variant}>{row.status}</StatusChip>;
      },
    },
    { key: "template", header: "Template", render: (row: ApplianceRowData) => row.template },
    { key: "networks", header: "Networks", render: (row: ApplianceRowData) => row.networks.join(", ") || "—" },
    { key: "created", header: "Created", render: (row: ApplianceRowData) => row.created },
    {
      key: "actions",
      header: "",
      render: (row: ApplianceRowData) => (
        <Button variant="ghost" size="sm" onClick={() => navigate(`/workspaces/${workspaceId}/appliances/${row.id}`)}>
          View
        </Button>
      ),
    },
  ];

  // Loading state
  if (appliancesQuery.isLoading) {
    return (
      <div style={{ display: "flex", justifyContent: "center", alignItems: "center", height: 400 }}>
        <Spinner size="lg" />
      </div>
    );
  }

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Appliance Fleet"
        subtitle={workspaceId ? `Workspace: ${workspaceId}` : undefined}
        actions={
          <CapabilityGate allowed={true} reason="Requires create:appliance permission">
            <Button variant="primary" onClick={() => navigate(`/workspaces/${workspaceId}/appliances/new`)}>
              Create Appliance
            </Button>
          </CapabilityGate>
        }
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Appliances" },
        ]}
      />

      {/* Toolbar */}
      <div style={{ display: "flex", gap: 12, marginBottom: 16, alignItems: "center", flexWrap: "wrap" }}>
        <SearchInput value={search} onChange={setSearch} placeholder="Search appliances..." />

        <FilterBar>
          <FilterChip label="All" active={statusFilter === "all"} onToggle={() => setStatusFilter("all")} />
          <FilterChip label="Running" active={statusFilter === "running"} onToggle={() => setStatusFilter("running")} />
          <FilterChip label="Stopped" active={statusFilter === "stopped"} onToggle={() => setStatusFilter("stopped")} />
          <FilterChip label="Error" active={statusFilter === "error"} onToggle={() => setStatusFilter("error")} />
        </FilterBar>

        {templates.length > 0 && (
          <select
            value={templateFilter ?? ""}
            onChange={(e) => setTemplateFilter(e.target.value || null)}
            style={{ padding: "6px 12px", borderRadius: 6 }}
          >
            <option value="">All Templates</option>
            {templates.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        )}

        <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
          <Button variant={viewMode === "table" ? "primary" : "secondary"} size="sm" onClick={() => setViewMode("table")}>
            Table
          </Button>
          <Button variant={viewMode === "cards" ? "primary" : "secondary"} size="sm" onClick={() => setViewMode("cards")}>
            Cards
          </Button>
        </div>
      </div>

      {/* Bulk Action Bar */}
      {selection.size > 0 && (
        <BulkActionBar count={selection.size} onClear={clearSelection}>
          <Button variant="secondary" size="sm" onClick={() => setConfirmBulkStop(true)}>
            Stop Selected
          </Button>
          <Button variant="danger" size="sm" onClick={() => setConfirmBulkDelete(true)}>
            Delete Selected
          </Button>
        </BulkActionBar>
      )}

      {/* Content */}
      {filtered.length === 0 ? (
        <EmptyState
          title="No appliances found"
          description={search || statusFilter !== "all" ? "Try adjusting your filters" : "Create your first appliance to get started"}
          action={
            <Button variant="primary" onClick={() => navigate(`/workspaces/${workspaceId}/appliances/new`)}>
              Create Appliance
            </Button>
          }
        />
      ) : viewMode === "table" ? (
        <Table columns={columns} data={rows} getRowKey={(r) => r.id} />
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: 16 }}>
          {filtered.map((a) => (
            <ApplianceCard
              key={a.id}
              id={a.id}
              name={a.name}
              status={a.status}
              template={a.templateId}
              created={new Date(a.createdAt).toLocaleDateString()}
              selected={selection.has(a.id)}
              onSelect={toggleSelect}
              onClick={() => navigate(`/workspaces/${workspaceId}/appliances/${a.id}`)}
            />
          ))}
        </div>
      )}

      {/* Confirm Dialogs */}
      <ConfirmDialog
        open={confirmBulkStop}
        title="Stop Selected Appliances"
        description={`Are you sure you want to stop ${selection.size} appliance(s)?`}
        confirmLabel="Stop"
        tone="warning"
        onConfirm={handleBulkStop}
        onCancel={() => setConfirmBulkStop(false)}
        loading={bulkBusy}
      />

      <ConfirmDialog
        open={confirmBulkDelete}
        title="Delete Selected Appliances"
        description={`Are you sure you want to delete ${selection.size} appliance(s)? This action cannot be undone.`}
        confirmLabel="Delete"
        tone="danger"
        onConfirm={handleBulkDelete}
        onCancel={() => setConfirmBulkDelete(false)}
        loading={bulkBusy}
      />
    </div>
  );
}

export default ApplianceFleet;
