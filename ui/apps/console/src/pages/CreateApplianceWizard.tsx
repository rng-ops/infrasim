import React, { useState, useMemo } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  PageHeader,
  Button,
  Card,
  StepWizard,
  Step,
  FormField,
  Input,
  Select,
  Checkbox,
  PropertyGrid,
  ErrorSummary,
  Spinner,
  DiffList,
  DiffItem,
} from "@infrasim/ui";
import { useApi } from "../api-context";

// ============================================================================
// Step 1: Template Selection
// ============================================================================

interface TemplatePickerProps {
  templates: Array<{ id: string; name: string; description: string; category: string }>;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

function TemplatePicker({ templates, selectedId, onSelect }: TemplatePickerProps) {
  const categories = useMemo(() => {
    const cats = new Set<string>();
    templates.forEach((t) => cats.add(t.category));
    return Array.from(cats);
  }, [templates]);

  const [categoryFilter, setCategoryFilter] = useState<string | null>(null);
  const filtered = categoryFilter ? templates.filter((t) => t.category === categoryFilter) : templates;

  return (
    <div>
      <div style={{ marginBottom: 16, display: "flex", gap: 8 }}>
        <Button variant={categoryFilter === null ? "primary" : "secondary"} size="sm" onClick={() => setCategoryFilter(null)}>
          All
        </Button>
        {categories.map((cat) => (
          <Button key={cat} variant={categoryFilter === cat ? "primary" : "secondary"} size="sm" onClick={() => setCategoryFilter(cat)}>
            {cat}
          </Button>
        ))}
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(250px, 1fr))", gap: 12 }}>
        {filtered.map((t) => (
          <Card
            key={t.id}
            style={{
              cursor: "pointer",
              border: selectedId === t.id ? "2px solid var(--ifm-color-primary)" : "1px solid var(--ifm-color-border)",
              transition: "border-color 0.15s",
            }}
            onClick={() => onSelect(t.id)}
          >
            <h4 style={{ margin: "0 0 8px 0" }}>{t.name}</h4>
            <p style={{ margin: 0, fontSize: 13, color: "var(--ifm-color-subtle)" }}>{t.description}</p>
            <div style={{ marginTop: 8, fontSize: 11, color: "var(--ifm-color-muted)" }}>{t.category}</div>
          </Card>
        ))}
      </div>

      {filtered.length === 0 && <p style={{ color: "var(--ifm-color-subtle)" }}>No templates available</p>}
    </div>
  );
}

// ============================================================================
// Step 2: Configuration
// ============================================================================

interface ConfigurationStepProps {
  name: string;
  setName: (v: string) => void;
  memory: number;
  setMemory: (v: number) => void;
  cpus: number;
  setCpus: (v: number) => void;
  diskSize: number;
  setDiskSize: (v: number) => void;
}

function ConfigurationStep({ name, setName, memory, setMemory, cpus, setCpus, diskSize, setDiskSize }: ConfigurationStepProps) {
  return (
    <div style={{ maxWidth: 500 }}>
      <FormField label="Appliance Name" required hint="A unique name for this appliance">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-appliance" />
      </FormField>

      <FormField label="Memory (MB)" required>
        <Input type="number" value={memory} onChange={(e) => setMemory(parseInt(e.target.value) || 0)} min={256} step={256} />
      </FormField>

      <FormField label="CPUs" required>
        <Input type="number" value={cpus} onChange={(e) => setCpus(parseInt(e.target.value) || 1)} min={1} max={32} />
      </FormField>

      <FormField label="Disk Size (GB)" required>
        <Input type="number" value={diskSize} onChange={(e) => setDiskSize(parseInt(e.target.value) || 10)} min={1} />
      </FormField>
    </div>
  );
}

// ============================================================================
// Step 3: Network Attachment
// ============================================================================

interface NetworkAttachStepProps {
  networks: Array<{ id: string; name: string; cidr: string }>;
  selectedNetworks: string[];
  onToggle: (id: string) => void;
}

function NetworkAttachStep({ networks, selectedNetworks, onToggle }: NetworkAttachStepProps) {
  return (
    <div>
      <p style={{ marginBottom: 16, color: "var(--ifm-color-subtle)" }}>
        Select the networks this appliance should be connected to.
      </p>

      {networks.length === 0 ? (
        <p>No networks available in this workspace.</p>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {networks.map((n) => (
            <Card key={n.id} style={{ padding: 12 }}>
              <Checkbox checked={selectedNetworks.includes(n.id)} onChange={() => onToggle(n.id)} label={n.name} />
              <div style={{ marginLeft: 24, fontSize: 12, color: "var(--ifm-color-subtle)" }}>CIDR: {n.cidr}</div>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Step 4: Review Plan
// ============================================================================

interface ReviewPlanProps {
  templateName: string;
  name: string;
  memory: number;
  cpus: number;
  diskSize: number;
  networks: string[];
}

function ReviewPlan({ templateName, name, memory, cpus, diskSize, networks }: ReviewPlanProps) {
  const diffItems: DiffItem[] = [
    { type: "add", name, resourceType: "appliance", changes: [`Template: ${templateName}`, `Memory: ${memory}MB`, `CPUs: ${cpus}`, `Disk: ${diskSize}GB`] },
    ...networks.map((n) => ({ type: "add" as const, name: `${name}-nic-${n}`, resourceType: "network_interface", changes: [`Network: ${n}`] })),
  ];

  return (
    <div>
      <h4 style={{ marginBottom: 16 }}>Review Your Configuration</h4>

      <PropertyGrid
        rows={[
          { label: "Name", value: name },
          { label: "Template", value: templateName },
          { label: "Memory", value: `${memory} MB` },
          { label: "CPUs", value: cpus },
          { label: "Disk Size", value: `${diskSize} GB` },
          { label: "Networks", value: networks.length > 0 ? networks.join(", ") : "None" },
        ]}
      />

      <h4 style={{ marginTop: 24, marginBottom: 12 }}>Planned Changes</h4>
      <DiffList items={diffItems} />
    </div>
  );
}

// ============================================================================
// Main Wizard Component
// ============================================================================

export function CreateApplianceWizard() {
  const { workspaceId } = useParams<{ workspaceId: string }>();
  const navigate = useNavigate();
  const { hooks } = useApi();

  // Mock templates (would come from API)
  const templates = [
    { id: "ubuntu-22.04", name: "Ubuntu 22.04", description: "Standard Ubuntu LTS server", category: "Linux" },
    { id: "debian-12", name: "Debian 12", description: "Stable Debian server", category: "Linux" },
    { id: "alpine-3.18", name: "Alpine 3.18", description: "Lightweight Alpine Linux", category: "Linux" },
    { id: "windows-server-2022", name: "Windows Server 2022", description: "Windows Server datacenter", category: "Windows" },
    { id: "freebsd-14", name: "FreeBSD 14", description: "FreeBSD Unix", category: "BSD" },
    { id: "router-vyos", name: "VyOS Router", description: "Network router appliance", category: "Network" },
    { id: "firewall-pfsense", name: "pfSense Firewall", description: "Firewall appliance", category: "Network" },
  ];

  // Mock networks (would come from API)
  const networks = [
    { id: "net-mgmt", name: "Management", cidr: "10.0.0.0/24" },
    { id: "net-app", name: "Application", cidr: "10.0.1.0/24" },
    { id: "net-dmz", name: "DMZ", cidr: "10.0.2.0/24" },
  ];

  // Wizard state
  const [selectedTemplate, setSelectedTemplate] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [memory, setMemory] = useState(2048);
  const [cpus, setCpus] = useState(2);
  const [diskSize, setDiskSize] = useState(20);
  const [selectedNetworks, setSelectedNetworks] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleNetwork = (id: string) => {
    setSelectedNetworks((prev) => (prev.includes(id) ? prev.filter((n) => n !== id) : [...prev, id]));
  };

  const selectedTemplateName = templates.find((t) => t.id === selectedTemplate)?.name ?? "";

  const steps: Step[] = [
    {
      id: "template",
      label: "Select Template",
      content: <TemplatePicker templates={templates} selectedId={selectedTemplate} onSelect={setSelectedTemplate} />,
      validate: () => (selectedTemplate ? [] : ["Please select a template"]),
    },
    {
      id: "configure",
      label: "Configure",
      content: (
        <ConfigurationStep
          name={name}
          setName={setName}
          memory={memory}
          setMemory={setMemory}
          cpus={cpus}
          setCpus={setCpus}
          diskSize={diskSize}
          setDiskSize={setDiskSize}
        />
      ),
      validate: () => {
        const errors: string[] = [];
        if (!name.trim()) errors.push("Appliance name is required");
        if (name.length > 0 && !/^[a-z0-9-]+$/.test(name)) errors.push("Name must be lowercase alphanumeric with hyphens only");
        if (memory < 256) errors.push("Memory must be at least 256 MB");
        if (cpus < 1) errors.push("At least 1 CPU is required");
        if (diskSize < 1) errors.push("Disk size must be at least 1 GB");
        return errors;
      },
    },
    {
      id: "network",
      label: "Networks",
      content: <NetworkAttachStep networks={networks} selectedNetworks={selectedNetworks} onToggle={toggleNetwork} />,
      validate: () => [], // Networks are optional
    },
    {
      id: "review",
      label: "Review & Create",
      content: (
        <ReviewPlan templateName={selectedTemplateName} name={name} memory={memory} cpus={cpus} diskSize={diskSize} networks={selectedNetworks} />
      ),
    },
  ];

  async function handleFinish() {
    setCreating(true);
    setError(null);

    try {
      // TODO: Call actual API
      await new Promise((r) => setTimeout(r, 1500));

      // Navigate back to fleet view
      navigate(`/workspaces/${workspaceId}/appliances`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create appliance");
      setCreating(false);
    }
  }

  return (
    <div style={{ padding: 24, maxWidth: 900, margin: "0 auto" }}>
      <PageHeader
        title="Create Appliance"
        subtitle="Configure and deploy a new appliance"
        breadcrumbs={[
          { label: "Home", href: "/" },
          { label: "Workspaces", href: "/workspaces" },
          { label: workspaceId ?? "Workspace", href: `/workspaces/${workspaceId}` },
          { label: "Appliances", href: `/workspaces/${workspaceId}/appliances` },
          { label: "Create" },
        ]}
      />

      {error && <ErrorSummary errors={[{ message: error }]} />}

      {creating ? (
        <Card style={{ padding: 48, textAlign: "center" }}>
          <Spinner size="lg" />
          <p style={{ marginTop: 16, color: "var(--ifm-color-subtle)" }}>Creating appliance...</p>
        </Card>
      ) : (
        <Card style={{ padding: 24 }}>
          <StepWizard steps={steps} onFinish={handleFinish} />
        </Card>
      )}

      <div style={{ marginTop: 16 }}>
        <Button variant="ghost" onClick={() => navigate(`/workspaces/${workspaceId}/appliances`)}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

export default CreateApplianceWizard;
