import React, { useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Table,
  StatusChip,
  StepWizard,
  Step,
  FormField,
  Input,
  Select,
  Checkbox,
  PropertyGrid,
  Stepper,
  StepperItem,
  ProgressBar,
  ConfirmDialog,
  EmptyState,
  Spinner,
  Dialog,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Snapshot Types
// ============================================================================

interface Snapshot {
  id: string;
  name: string;
  applianceId: string;
  applianceName: string;
  createdAt: string;
  size: string;
  status: "ready" | "creating" | "error";
  type: "manual" | "scheduled";
}

// ============================================================================
// Create Snapshot Dialog
// ============================================================================

interface CreateSnapshotDialogProps {
  open: boolean;
  onClose: () => void;
  appliances: Array<{ id: string; name: string }>;
}

function CreateSnapshotDialog({ open, onClose, appliances }: CreateSnapshotDialogProps) {
  const [name, setName] = useState("");
  const [applianceId, setApplianceId] = useState("");
  const [includeMemory, setIncludeMemory] = useState(false);
  const [creating, setCreating] = useState(false);

  async function handleCreate() {
    setCreating(true);
    await new Promise((r) => setTimeout(r, 2000));
    setCreating(false);
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Create Snapshot"
      description="Create a point-in-time snapshot of an appliance"
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={creating}>Cancel</Button>
          <Button variant="primary" onClick={handleCreate} loading={creating} disabled={!name || !applianceId}>
            Create Snapshot
          </Button>
        </>
      }
    >
      <FormField label="Snapshot Name" required>
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-snapshot" />
      </FormField>

      <FormField label="Appliance" required>
        <Select value={applianceId} onChange={(e) => setApplianceId(e.target.value)}>
          <option value="">Select an appliance...</option>
          {appliances.map((a) => (
            <option key={a.id} value={a.id}>{a.name}</option>
          ))}
        </Select>
      </FormField>

      <div style={{ marginTop: 12 }}>
        <Checkbox checked={includeMemory} onChange={setIncludeMemory} label="Include memory state (hibernation snapshot)" />
      </div>
    </Dialog>
  );
}

// ============================================================================
// Restore Wizard
// ============================================================================

interface RestoreWizardProps {
  snapshot: Snapshot;
  onClose: () => void;
  onComplete: () => void;
}

function RestoreWizard({ snapshot, onClose, onComplete }: RestoreWizardProps) {
  const [targetType, setTargetType] = useState<"new" | "existing">("new");
  const [newName, setNewName] = useState(`${snapshot.applianceName}-restored`);
  const [targetAppliance, setTargetAppliance] = useState("");
  const [restoring, setRestoring] = useState(false);
  const [progress, setProgress] = useState(0);
  const [steps, setSteps] = useState<StepperItem[]>([]);

  const wizardSteps: Step[] = [
    {
      id: "target",
      label: "Select Target",
      content: (
        <div>
          <FormField label="Restore To">
            <div style={{ display: "flex", gap: 12, marginTop: 8 }}>
              <Button
                variant={targetType === "new" ? "primary" : "secondary"}
                onClick={() => setTargetType("new")}
              >
                New Appliance
              </Button>
              <Button
                variant={targetType === "existing" ? "primary" : "secondary"}
                onClick={() => setTargetType("existing")}
              >
                Existing Appliance
              </Button>
            </div>
          </FormField>

          {targetType === "new" && (
            <FormField label="New Appliance Name" required>
              <Input value={newName} onChange={(e) => setNewName(e.target.value)} />
            </FormField>
          )}

          {targetType === "existing" && (
            <FormField label="Target Appliance" required hint="Warning: This will overwrite the existing appliance">
              <Select value={targetAppliance} onChange={(e) => setTargetAppliance(e.target.value)}>
                <option value="">Select appliance...</option>
                <option value="web-server-2">web-server-2</option>
                <option value="test-vm">test-vm</option>
              </Select>
            </FormField>
          )}
        </div>
      ),
      validate: () => {
        if (targetType === "new" && !newName) return ["New appliance name is required"];
        if (targetType === "existing" && !targetAppliance) return ["Target appliance is required"];
        return [];
      },
    },
    {
      id: "review",
      label: "Review",
      content: (
        <div>
          <h4>Review Restore Operation</h4>
          <PropertyGrid
            rows={[
              { label: "Source Snapshot", value: snapshot.name },
              { label: "Original Appliance", value: snapshot.applianceName },
              { label: "Snapshot Date", value: new Date(snapshot.createdAt).toLocaleString() },
              { label: "Snapshot Size", value: snapshot.size },
              { label: "Target Type", value: targetType === "new" ? "New Appliance" : "Existing Appliance" },
              { label: "Target Name", value: targetType === "new" ? newName : targetAppliance },
            ]}
          />

          {targetType === "existing" && (
            <Card style={{ marginTop: 16, padding: 12, background: "var(--ifm-color-warning-bg)" }}>
              <strong>⚠️ Warning:</strong> Restoring to an existing appliance will overwrite all its data.
            </Card>
          )}
        </div>
      ),
    },
  ];

  async function handleFinish() {
    setRestoring(true);
    
    const restoreSteps: StepperItem[] = [
      { id: "1", label: "Preparing restore", status: "pending" },
      { id: "2", label: "Copying disk data", status: "pending" },
      { id: "3", label: "Configuring appliance", status: "pending" },
      { id: "4", label: "Starting appliance", status: "pending" },
    ];
    setSteps(restoreSteps);

    // Simulate restore progress
    for (let i = 0; i < restoreSteps.length; i++) {
      setSteps((prev) =>
        prev.map((s, idx) => ({
          ...s,
          status: idx < i ? "completed" : idx === i ? "running" : "pending",
        }))
      );

      // Progress for disk copy step
      if (i === 1) {
        for (let p = 0; p <= 100; p += 10) {
          await new Promise((r) => setTimeout(r, 200));
          setProgress(p);
        }
      } else {
        await new Promise((r) => setTimeout(r, 1000));
      }
    }

    setSteps((prev) => prev.map((s) => ({ ...s, status: "completed" })));
    await new Promise((r) => setTimeout(r, 500));
    
    onComplete();
  }

  if (restoring) {
    return (
      <Card style={{ padding: 24 }}>
        <h3 style={{ marginTop: 0 }}>Restoring Snapshot</h3>
        <Stepper steps={steps} />
        
        {steps.find((s) => s.status === "running")?.id === "2" && (
          <div style={{ marginTop: 16 }}>
            <ProgressBar value={progress} label="Copying disk data" />
            <div style={{ textAlign: "center", marginTop: 8, color: "var(--ifm-color-subtle)" }}>
              {progress}% complete
            </div>
          </div>
        )}
      </Card>
    );
  }

  return (
    <Card style={{ padding: 24 }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
        <h3 style={{ margin: 0 }}>Restore Snapshot: {snapshot.name}</h3>
        <Button variant="ghost" onClick={onClose}>×</Button>
      </div>
      <StepWizard steps={wizardSteps} onFinish={handleFinish} />
    </Card>
  );
}

// ============================================================================
// Main SnapshotRestore Component
// ============================================================================

export function SnapshotRestore() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const navigate = useNavigate();
  const { hooks } = useApi();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [restoreSnapshot, setRestoreSnapshot] = useState<Snapshot | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Snapshot | null>(null);
  const [deleting, setDeleting] = useState(false);

  // Mock snapshots
  const snapshots: Snapshot[] = [
    { id: "snap-1", name: "pre-upgrade", applianceId: "app-1", applianceName: "web-server-1", createdAt: new Date(Date.now() - 3600000).toISOString(), size: "4.2 GB", status: "ready", type: "manual" },
    { id: "snap-2", name: "daily-backup-001", applianceId: "app-2", applianceName: "db-primary", createdAt: new Date(Date.now() - 86400000).toISOString(), size: "12.8 GB", status: "ready", type: "scheduled" },
    { id: "snap-3", name: "test-snapshot", applianceId: "app-1", applianceName: "web-server-1", createdAt: new Date(Date.now() - 172800000).toISOString(), size: "3.9 GB", status: "ready", type: "manual" },
    { id: "snap-4", name: "backup-in-progress", applianceId: "app-3", applianceName: "cache-node", createdAt: new Date().toISOString(), size: "—", status: "creating", type: "manual" },
  ];

  // Mock appliances for create dialog
  const appliances = [
    { id: "app-1", name: "web-server-1" },
    { id: "app-2", name: "db-primary" },
    { id: "app-3", name: "cache-node" },
  ];

  async function handleDelete() {
    if (!confirmDelete) return;
    setDeleting(true);
    await new Promise((r) => setTimeout(r, 1000));
    setDeleting(false);
    setConfirmDelete(null);
  }

  // Table columns
  const columns = [
    { key: "name", header: "Name", render: (s: Snapshot) => s.name },
    { key: "appliance", header: "Appliance", render: (s: Snapshot) => s.applianceName },
    { 
      key: "status", 
      header: "Status", 
      render: (s: Snapshot) => (
        <StatusChip status={s.status === "ready" ? "success" : s.status === "creating" ? "warning" : "danger"}>
          {s.status}
        </StatusChip>
      ) 
    },
    { key: "type", header: "Type", render: (s: Snapshot) => <StatusChip status="muted">{s.type}</StatusChip> },
    { key: "size", header: "Size", render: (s: Snapshot) => s.size },
    { key: "created", header: "Created", render: (s: Snapshot) => new Date(s.createdAt).toLocaleString() },
    {
      key: "actions",
      header: "",
      render: (s: Snapshot) => (
        <div style={{ display: "flex", gap: 4 }}>
          <Button variant="ghost" size="sm" onClick={() => setRestoreSnapshot(s)} disabled={s.status !== "ready"}>
            Restore
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setConfirmDelete(s)} disabled={s.status === "creating"}>
            Delete
          </Button>
        </div>
      ),
    },
  ];

  // Show restore wizard if active
  if (restoreSnapshot) {
    return (
      <div style={{ padding: 24, maxWidth: 800, margin: "0 auto" }}>
        <PageHeader
          title="Restore Snapshot"
          breadcrumbs={[
            { label: "Home", href: "/" },
            { label: "Workspaces", href: "/workspaces" },
            { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
            { label: "Snapshots", href: `/workspaces/${workspaceId}/snapshots` },
            { label: "Restore" },
          ]}
        />
        <RestoreWizard
          snapshot={restoreSnapshot}
          onClose={() => setRestoreSnapshot(null)}
          onComplete={() => {
            setRestoreSnapshot(null);
            // Could navigate to the new appliance here
          }}
        />
      </div>
    );
  }

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Snapshots"
        subtitle="Manage point-in-time snapshots for backup and restore"
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Snapshots" },
        ]}
        actions={
          <Button variant="primary" onClick={() => setCreateDialogOpen(true)}>
            Create Snapshot
          </Button>
        }
      />

      {snapshots.length === 0 ? (
        <EmptyState
          title="No snapshots"
          description="Create your first snapshot to enable point-in-time recovery"
          action={<Button variant="primary" onClick={() => setCreateDialogOpen(true)}>Create Snapshot</Button>}
        />
      ) : (
        <Card style={{ padding: 16 }}>
          <Table columns={columns} data={snapshots} getRowKey={(s) => s.id} />
        </Card>
      )}

      <CreateSnapshotDialog
        open={createDialogOpen}
        onClose={() => setCreateDialogOpen(false)}
        appliances={appliances}
      />

      <ConfirmDialog
        open={confirmDelete !== null}
        title="Delete Snapshot"
        description={`Are you sure you want to delete "${confirmDelete?.name}"? This action cannot be undone.`}
        confirmLabel="Delete"
        tone="danger"
        onConfirm={handleDelete}
        onCancel={() => setConfirmDelete(null)}
        loading={deleting}
      />
    </div>
  );
}

export default SnapshotRestore;
