import React, { useState } from "react";
import { useParams } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Tabs,
  TabItem,
  StatusChip,
  PropertyGrid,
  DiffList,
  DiffItem,
  Stepper,
  StepperItem,
  ConfirmDialog,
  Spinner,
  Panel,
  DockLayout,
  ErrorSummary,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// State View (current infrastructure state)
// ============================================================================

interface StateResource {
  id: string;
  type: string;
  name: string;
  provider: string;
  attributes: Record<string, string>;
}

function StateView({ resources }: { resources: StateResource[] }) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = resources.find((r) => r.id === selectedId);

  return (
    <div style={{ display: "flex", gap: 16 }}>
      <div style={{ flex: 1 }}>
        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Type</th>
              <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Name</th>
              <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Provider</th>
            </tr>
          </thead>
          <tbody>
            {resources.map((r) => (
              <tr
                key={r.id}
                onClick={() => setSelectedId(r.id)}
                style={{ cursor: "pointer", background: selectedId === r.id ? "var(--ifm-color-primary-bg)" : undefined }}
              >
                <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>{r.type}</td>
                <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>{r.name}</td>
                <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>{r.provider}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {selected && (
        <Card style={{ width: 300, padding: 16 }}>
          <h4 style={{ marginTop: 0 }}>{selected.name}</h4>
          <PropertyGrid
            rows={Object.entries(selected.attributes).map(([k, v]) => ({ label: k, value: v }))}
          />
        </Card>
      )}
    </div>
  );
}

// ============================================================================
// Plan View (diff between desired and current)
// ============================================================================

interface PlanViewProps {
  items: DiffItem[];
  onApply: () => void;
  applyDisabled: boolean;
}

function PlanView({ items, onApply, applyDisabled }: PlanViewProps) {
  const addCount = items.filter((i) => i.type === "add").length;
  const updateCount = items.filter((i) => i.type === "update").length;
  const deleteCount = items.filter((i) => i.type === "delete").length;

  return (
    <div>
      <div style={{ display: "flex", gap: 16, marginBottom: 16, alignItems: "center" }}>
        <div>
          <strong>{addCount}</strong> to add, <strong>{updateCount}</strong> to update, <strong>{deleteCount}</strong> to delete
        </div>
        <div style={{ marginLeft: "auto" }}>
          <Button variant="primary" onClick={onApply} disabled={applyDisabled || items.length === 0}>
            Apply Changes
          </Button>
        </div>
      </div>

      {items.length === 0 ? (
        <Card style={{ padding: 24, textAlign: "center" }}>
          <p style={{ color: "var(--ifm-color-success)" }}>✓ Infrastructure is up to date</p>
        </Card>
      ) : (
        <DiffList items={items} />
      )}
    </div>
  );
}

// ============================================================================
// Apply Progress View
// ============================================================================

interface ApplyProgressProps {
  steps: StepperItem[];
  logs: string[];
  error?: string;
}

function ApplyProgress({ steps, logs, error }: ApplyProgressProps) {
  return (
    <div>
      <Stepper steps={steps} />

      {error && <ErrorSummary errors={[{ message: error }]} />}

      <Card style={{ marginTop: 16, padding: 8, background: "#0d0d0d", maxHeight: 300, overflow: "auto" }}>
        <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 12, color: "#0f0" }}>
          {logs.join("\n")}
        </pre>
      </Card>
    </div>
  );
}

// ============================================================================
// Main TerraformSync Component
// ============================================================================

export function TerraformSync() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const { hooks } = useApi();

  const [activeTab, setActiveTab] = useState("plan");
  const [confirmApply, setConfirmApply] = useState(false);
  const [applying, setApplying] = useState(false);
  const [applySteps, setApplySteps] = useState<StepperItem[]>([]);
  const [applyLogs, setApplyLogs] = useState<string[]>([]);
  const [applyError, setApplyError] = useState<string | undefined>();
  const [refreshing, setRefreshing] = useState(false);

  // Mock state resources
  const stateResources: StateResource[] = [
    { id: "1", type: "infrasim_appliance", name: "web-server-1", provider: "infrasim", attributes: { memory: "4096", cpus: "4", status: "running" } },
    { id: "2", type: "infrasim_appliance", name: "db-primary", provider: "infrasim", attributes: { memory: "8192", cpus: "8", status: "running" } },
    { id: "3", type: "infrasim_network", name: "management", provider: "infrasim", attributes: { cidr: "10.0.0.0/24", type: "bridge" } },
    { id: "4", type: "infrasim_network", name: "application", provider: "infrasim", attributes: { cidr: "10.0.1.0/24", type: "nat" } },
    { id: "5", type: "infrasim_volume", name: "data-vol-1", provider: "infrasim", attributes: { size: "100GB", attached_to: "db-primary" } },
  ];

  // Mock plan diff
  const planItems: DiffItem[] = [
    { type: "add", name: "cache-node", resourceType: "infrasim_appliance", changes: ["memory: 2048", "cpus: 2", "template: redis"] },
    { type: "update", name: "web-server-1", resourceType: "infrasim_appliance", changes: ["memory: 4096 → 8192", "cpus: 4 → 8"] },
    { type: "add", name: "cache-network", resourceType: "infrasim_network", changes: ["cidr: 10.0.5.0/24", "type: isolated"] },
  ];

  // Tabs
  const tabs: TabItem[] = [
    { id: "plan", label: "Plan" },
    { id: "state", label: "State" },
    { id: "configuration", label: "Configuration" },
    { id: "history", label: "History" },
  ];

  // Refresh state
  async function handleRefresh() {
    setRefreshing(true);
    await new Promise((r) => setTimeout(r, 1500));
    setRefreshing(false);
  }

  // Apply changes
  async function handleApply() {
    setConfirmApply(false);
    setApplying(true);
    setActiveTab("plan");
    setApplyError(undefined);

    const steps: StepperItem[] = [
      { id: "1", label: "Validating configuration", status: "pending" },
      { id: "2", label: "Creating cache-network", status: "pending" },
      { id: "3", label: "Creating cache-node", status: "pending" },
      { id: "4", label: "Updating web-server-1", status: "pending" },
      { id: "5", label: "Finalizing", status: "pending" },
    ];

    setApplySteps(steps);
    setApplyLogs(["Starting apply..."]);

    // Simulate progress
    for (let i = 0; i < steps.length; i++) {
      await new Promise((r) => setTimeout(r, 1000));

      setApplySteps((prev) =>
        prev.map((s, idx) => ({
          ...s,
          status: idx < i ? "completed" : idx === i ? "running" : "pending",
        }))
      );

      setApplyLogs((prev) => [...prev, `[${new Date().toISOString()}] ${steps[i].label}...`]);
    }

    // Complete
    await new Promise((r) => setTimeout(r, 500));
    setApplySteps((prev) => prev.map((s) => ({ ...s, status: "completed" })));
    setApplyLogs((prev) => [...prev, "Apply complete! 2 added, 1 changed, 0 destroyed."]);
    setApplying(false);
  }

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Terraform Sync"
        subtitle="Manage infrastructure as code"
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Terraform" },
        ]}
        actions={
          <div style={{ display: "flex", gap: 8 }}>
            <Button variant="secondary" onClick={handleRefresh} disabled={refreshing} loading={refreshing}>
              Refresh State
            </Button>
            <Button variant="primary" disabled={applying}>
              Import Resource
            </Button>
          </div>
        }
      />

      <DockLayout
        center={
          <Card style={{ padding: 16 }}>
            <Tabs tabs={tabs} activeId={activeTab} onChange={setActiveTab} />

            <div style={{ marginTop: 16 }}>
              {activeTab === "plan" && (
                applying ? (
                  <ApplyProgress steps={applySteps} logs={applyLogs} error={applyError} />
                ) : (
                  <PlanView items={planItems} onApply={() => setConfirmApply(true)} applyDisabled={applying} />
                )
              )}

              {activeTab === "state" && <StateView resources={stateResources} />}

              {activeTab === "configuration" && (
                <Card style={{ padding: 16, background: "#1e1e1e" }}>
                  <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 13, color: "#d4d4d4", overflow: "auto" }}>
{`# Terraform configuration
terraform {
  required_providers {
    infrasim = {
      source = "infrasim/infrasim"
      version = "~> 1.0"
    }
  }
}

provider "infrasim" {
  endpoint = "http://localhost:8080"
}

resource "infrasim_appliance" "web_server" {
  name     = "web-server-1"
  template = "ubuntu-22.04"
  memory   = 8192
  cpus     = 8

  network {
    name = infrasim_network.management.name
  }
}

resource "infrasim_network" "management" {
  name = "management"
  cidr = "10.0.0.0/24"
  type = "bridge"
}`}
                  </pre>
                </Card>
              )}

              {activeTab === "history" && (
                <table style={{ width: "100%", borderCollapse: "collapse" }}>
                  <thead>
                    <tr>
                      <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Run ID</th>
                      <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Status</th>
                      <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Changes</th>
                      <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Time</th>
                      <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>User</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>run-abc123</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}><StatusChip status="success">Applied</StatusChip></td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>+2 ~1 -0</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>5 min ago</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>admin</td>
                    </tr>
                    <tr>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>run-def456</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}><StatusChip status="success">Applied</StatusChip></td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>+5 ~0 -0</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>1 hour ago</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>admin</td>
                    </tr>
                    <tr>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>run-ghi789</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}><StatusChip status="danger">Failed</StatusChip></td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>+1 ~0 -0</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>2 hours ago</td>
                      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>user1</td>
                    </tr>
                  </tbody>
                </table>
              )}
            </div>
          </Card>
        }
        right={
          <Panel title="Quick Info">
            <PropertyGrid
              rows={[
                { label: "Workspace", value: workspaceId ?? "—" },
                { label: "Resources", value: stateResources.length },
                { label: "Last Refresh", value: "2 min ago" },
                { label: "Provider", value: "infrasim v1.2.0" },
                { label: "Terraform", value: "v1.6.0" },
              ]}
            />

            <div style={{ marginTop: 16 }}>
              <h5>Drift Detection</h5>
              <StatusChip status="success">No drift detected</StatusChip>
            </div>
          </Panel>
        }
      />

      <ConfirmDialog
        open={confirmApply}
        title="Apply Changes"
        description={`This will apply ${planItems.length} changes to your infrastructure. This action may take several minutes.`}
        confirmLabel="Apply"
        tone="warning"
        onConfirm={handleApply}
        onCancel={() => setConfirmApply(false)}
      />
    </div>
  );
}

export default TerraformSync;
