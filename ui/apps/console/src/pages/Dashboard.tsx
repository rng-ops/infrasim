import React from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, CodeBlock, PageHeader, StatusChip } from "@infrasim/ui";
import { useQuery } from "@tanstack/react-query";

export default function Dashboard({ client }: { client: ReturnType<typeof createApiClient> }) {
  const { data: daemon } = client.hooks.useDaemonStatus();
  const { data: vms } = client.hooks.useVms();

  return (
    <div>
      <PageHeader
        title="Dashboard"
        description="Monitor your virtual machines and appliances."
        actions={<Button variant="secondary" onClick={() => window.location.reload()}>Refresh</Button>}
      />
      <div className="grid">
        <Card title="Virtual Machines" actions={<Button variant="ghost" onClick={() => (window.location.href = "/vms")}>View VMs</Button>}>
          <p>{vms ? `${vms.filter(v => v.state === "running").length} running / ${vms.length} total` : "Loading..."}</p>
          {vms && vms.slice(0, 3).map(vm => (
            <div key={vm.id} className="vm-row">
              <span>{vm.name}</span>
              <StatusChip tone={vm.state === "running" ? "success" : "muted"} label={vm.state} />
            </div>
          ))}
          {!vms?.length && <p>No VMs running. Create from template.</p>}
        </Card>
        <Card title="Quick Start" actions={null}>
          <p>Use curl to check health:</p>
          <CodeBlock code={`curl -s http://127.0.0.1:8080/api/health`} />
        </Card>
        <Card title="System Status" actions={<Button variant="ghost" onClick={() => window.location.reload()}>Refresh</Button>}>
          {daemon ? (
            <ul className="status-list">
              <li><StatusChip tone={daemon.qemu_available ? "success" : "danger"} label={daemon.qemu_available ? "Daemon Online" : "Daemon Offline"} /></li>
              <li>QEMU {daemon.qemu_version}</li>
              <li>HVF {daemon.hvf_available ? "available" : "unavailable"}</li>
            </ul>
          ) : (
            <p>Loading status…</p>
          )}
        </Card>
      </div>
    </div>
  );
}

const style = document.createElement("style");
style.innerHTML = `
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 16px; }
.vm-row { display: flex; justify-content: space-between; align-items: center; padding: 6px 0; }
.status-list { list-style: none; padding: 0; margin: 0; color: #cbd5e1; }
`;
document.head.appendChild(style);
