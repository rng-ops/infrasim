import React, { useState, useMemo } from "react";
import { useParams } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  Table,
  StatusChip,
  Timeline,
  TimelineItem,
  FilterBar,
  FilterChip,
  SearchInput,
  PropertyGrid,
  Panel,
  DockLayout,
  Tabs,
  TabItem,
  EmptyState,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Event Types
// ============================================================================

interface AuditEvent {
  id: string;
  timestamp: string;
  type: "appliance" | "network" | "security" | "system" | "user";
  action: string;
  actor: string;
  resourceType: string;
  resourceId: string;
  resourceName: string;
  severity: "info" | "warning" | "error" | "success";
  details: Record<string, string>;
}

// ============================================================================
// Event Row Component
// ============================================================================

function EventRow({ event, selected, onSelect }: { event: AuditEvent; selected: boolean; onSelect: () => void }) {
  const severityColors = {
    info: "muted",
    warning: "warning",
    error: "danger",
    success: "success",
  } as const;

  const typeIcons = {
    appliance: "💻",
    network: "🌐",
    security: "🔒",
    system: "⚙️",
    user: "👤",
  };

  return (
    <tr
      onClick={onSelect}
      style={{
        cursor: "pointer",
        background: selected ? "var(--ifm-color-primary-bg)" : undefined,
      }}
    >
      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>
        {new Date(event.timestamp).toLocaleString()}
      </td>
      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>
        <span title={event.type}>{typeIcons[event.type]}</span> {event.action}
      </td>
      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>
        {event.resourceName}
      </td>
      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>
        {event.actor}
      </td>
      <td style={{ padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>
        <StatusChip status={severityColors[event.severity]}>{event.severity}</StatusChip>
      </td>
    </tr>
  );
}

// ============================================================================
// Main AuditTimeline Component
// ============================================================================

export function AuditTimeline() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const { hooks } = useApi();

  const [activeTab, setActiveTab] = useState("list");
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState<string | null>(null);
  const [severityFilter, setSeverityFilter] = useState<string | null>(null);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);

  // Mock events
  const events: AuditEvent[] = [
    {
      id: "e1",
      timestamp: new Date(Date.now() - 60000).toISOString(),
      type: "appliance",
      action: "Appliance started",
      actor: "admin",
      resourceType: "appliance",
      resourceId: "app-1",
      resourceName: "web-server-1",
      severity: "success",
      details: { previousState: "stopped", newState: "running" },
    },
    {
      id: "e2",
      timestamp: new Date(Date.now() - 300000).toISOString(),
      type: "network",
      action: "Network interface attached",
      actor: "admin",
      resourceType: "appliance",
      resourceId: "app-1",
      resourceName: "web-server-1",
      severity: "info",
      details: { interface: "eth1", network: "application" },
    },
    {
      id: "e3",
      timestamp: new Date(Date.now() - 600000).toISOString(),
      type: "security",
      action: "Trust attestation completed",
      actor: "system",
      resourceType: "appliance",
      resourceId: "app-2",
      resourceName: "db-primary",
      severity: "success",
      details: { attestationType: "TPM", result: "verified" },
    },
    {
      id: "e4",
      timestamp: new Date(Date.now() - 900000).toISOString(),
      type: "user",
      action: "User login",
      actor: "admin",
      resourceType: "session",
      resourceId: "sess-123",
      resourceName: "admin session",
      severity: "info",
      details: { ip: "192.168.1.100", method: "password" },
    },
    {
      id: "e5",
      timestamp: new Date(Date.now() - 1800000).toISOString(),
      type: "system",
      action: "Backup completed",
      actor: "system",
      resourceType: "workspace",
      resourceId: workspaceId ?? "ws-1",
      resourceName: "workspace backup",
      severity: "success",
      details: { size: "2.4 GB", duration: "45s" },
    },
    {
      id: "e6",
      timestamp: new Date(Date.now() - 3600000).toISOString(),
      type: "appliance",
      action: "Appliance creation failed",
      actor: "user1",
      resourceType: "appliance",
      resourceId: "app-failed",
      resourceName: "test-vm",
      severity: "error",
      details: { reason: "Insufficient resources", requestedMemory: "32GB", availableMemory: "16GB" },
    },
    {
      id: "e7",
      timestamp: new Date(Date.now() - 7200000).toISOString(),
      type: "security",
      action: "Permission denied",
      actor: "guest",
      resourceType: "appliance",
      resourceId: "app-2",
      resourceName: "db-primary",
      severity: "warning",
      details: { attemptedAction: "delete", requiredPermission: "admin" },
    },
    {
      id: "e8",
      timestamp: new Date(Date.now() - 86400000).toISOString(),
      type: "appliance",
      action: "Appliance created",
      actor: "admin",
      resourceType: "appliance",
      resourceId: "app-1",
      resourceName: "web-server-1",
      severity: "success",
      details: { template: "ubuntu-22.04", memory: "4096MB", cpus: "4" },
    },
  ];

  // Filter events
  const filtered = useMemo(() => {
    let list = events;
    if (search) {
      const q = search.toLowerCase();
      list = list.filter(
        (e) =>
          e.action.toLowerCase().includes(q) ||
          e.resourceName.toLowerCase().includes(q) ||
          e.actor.toLowerCase().includes(q)
      );
    }
    if (typeFilter) {
      list = list.filter((e) => e.type === typeFilter);
    }
    if (severityFilter) {
      list = list.filter((e) => e.severity === severityFilter);
    }
    return list;
  }, [events, search, typeFilter, severityFilter]);

  // Selected event
  const selectedEvent = events.find((e) => e.id === selectedEventId);

  // Timeline items for timeline view
  const timelineItems: TimelineItem[] = filtered.map((e) => ({
    id: e.id,
    time: new Date(e.timestamp).toLocaleTimeString(),
    title: e.action,
    description: `${e.resourceName} by ${e.actor}`,
    status: e.severity === "error" ? "danger" : e.severity === "success" ? "success" : "muted",
  }));

  // Tabs
  const tabs: TabItem[] = [
    { id: "list", label: "Event Log" },
    { id: "timeline", label: "Timeline" },
    { id: "stats", label: "Statistics" },
  ];

  // Stats
  const stats = useMemo(() => {
    const byType = events.reduce((acc, e) => {
      acc[e.type] = (acc[e.type] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);

    const bySeverity = events.reduce((acc, e) => {
      acc[e.severity] = (acc[e.severity] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);

    return { byType, bySeverity };
  }, [events]);

  return (
    <div style={{ padding: 24 }}>
      <PageHeader
        title="Audit Timeline"
        subtitle="View and search system events and user actions"
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Audit" },
        ]}
        actions={
          <Button variant="secondary" onClick={() => {}}>
            Export Logs
          </Button>
        }
      />

      <DockLayout
        center={
          <Card style={{ padding: 16 }}>
            <Tabs tabs={tabs} activeId={activeTab} onChange={setActiveTab} />

            {/* Filters */}
            <div style={{ display: "flex", gap: 12, marginTop: 16, marginBottom: 16, flexWrap: "wrap" }}>
              <SearchInput value={search} onChange={setSearch} placeholder="Search events..." />

              <FilterBar>
                <FilterChip label="All Types" active={typeFilter === null} onToggle={() => setTypeFilter(null)} />
                <FilterChip label="Appliance" active={typeFilter === "appliance"} onToggle={() => setTypeFilter("appliance")} />
                <FilterChip label="Network" active={typeFilter === "network"} onToggle={() => setTypeFilter("network")} />
                <FilterChip label="Security" active={typeFilter === "security"} onToggle={() => setTypeFilter("security")} />
                <FilterChip label="System" active={typeFilter === "system"} onToggle={() => setTypeFilter("system")} />
                <FilterChip label="User" active={typeFilter === "user"} onToggle={() => setTypeFilter("user")} />
              </FilterBar>

              <FilterBar>
                <FilterChip label="All Severity" active={severityFilter === null} onToggle={() => setSeverityFilter(null)} />
                <FilterChip label="Info" active={severityFilter === "info"} onToggle={() => setSeverityFilter("info")} />
                <FilterChip label="Success" active={severityFilter === "success"} onToggle={() => setSeverityFilter("success")} />
                <FilterChip label="Warning" active={severityFilter === "warning"} onToggle={() => setSeverityFilter("warning")} />
                <FilterChip label="Error" active={severityFilter === "error"} onToggle={() => setSeverityFilter("error")} />
              </FilterBar>
            </div>

            {/* Content */}
            <div style={{ marginTop: 16 }}>
              {activeTab === "list" && (
                filtered.length === 0 ? (
                  <EmptyState title="No events found" description="Try adjusting your filters" />
                ) : (
                  <table style={{ width: "100%", borderCollapse: "collapse" }}>
                    <thead>
                      <tr>
                        <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Time</th>
                        <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Action</th>
                        <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Resource</th>
                        <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Actor</th>
                        <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid var(--ifm-color-border)" }}>Severity</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filtered.map((e) => (
                        <EventRow
                          key={e.id}
                          event={e}
                          selected={e.id === selectedEventId}
                          onSelect={() => setSelectedEventId(e.id)}
                        />
                      ))}
                    </tbody>
                  </table>
                )
              )}

              {activeTab === "timeline" && <Timeline items={timelineItems} />}

              {activeTab === "stats" && (
                <div style={{ display: "grid", gridTemplateColumns: "repeat(2, 1fr)", gap: 24 }}>
                  <Card style={{ padding: 16 }}>
                    <h4 style={{ marginTop: 0 }}>Events by Type</h4>
                    {Object.entries(stats.byType).map(([type, count]) => (
                      <div key={type} style={{ display: "flex", justifyContent: "space-between", padding: "4px 0" }}>
                        <span style={{ textTransform: "capitalize" }}>{type}</span>
                        <strong>{count}</strong>
                      </div>
                    ))}
                  </Card>

                  <Card style={{ padding: 16 }}>
                    <h4 style={{ marginTop: 0 }}>Events by Severity</h4>
                    {Object.entries(stats.bySeverity).map(([severity, count]) => (
                      <div key={severity} style={{ display: "flex", justifyContent: "space-between", padding: "4px 0" }}>
                        <StatusChip status={severity === "error" ? "danger" : severity === "warning" ? "warning" : severity === "success" ? "success" : "muted"}>
                          {severity}
                        </StatusChip>
                        <strong>{count}</strong>
                      </div>
                    ))}
                  </Card>
                </div>
              )}
            </div>
          </Card>
        }
        right={
          selectedEvent && (
            <Panel title="Event Details" actions={<Button variant="ghost" size="sm" onClick={() => setSelectedEventId(null)}>×</Button>}>
              <PropertyGrid
                rows={[
                  { label: "Event ID", value: selectedEvent.id },
                  { label: "Timestamp", value: new Date(selectedEvent.timestamp).toLocaleString() },
                  { label: "Type", value: selectedEvent.type },
                  { label: "Action", value: selectedEvent.action },
                  { label: "Actor", value: selectedEvent.actor },
                  { label: "Resource", value: `${selectedEvent.resourceType}: ${selectedEvent.resourceName}` },
                  { label: "Severity", value: <StatusChip status={selectedEvent.severity === "error" ? "danger" : selectedEvent.severity === "warning" ? "warning" : selectedEvent.severity === "success" ? "success" : "muted"}>{selectedEvent.severity}</StatusChip> },
                ]}
              />

              <h5 style={{ marginTop: 16 }}>Details</h5>
              <PropertyGrid
                rows={Object.entries(selectedEvent.details).map(([k, v]) => ({ label: k, value: v }))}
              />
            </Panel>
          )
        }
      />
    </div>
  );
}

export default AuditTimeline;
