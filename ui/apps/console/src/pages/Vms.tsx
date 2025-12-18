import React from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, PageHeader, StatusChip, Table, EmptyState } from "@infrasim/ui";
import { useNavigate } from "react-router-dom";

export default function Vms({ client }: { client: ReturnType<typeof createApiClient> }) {
  const { data: vms, isLoading, error } = client.hooks.useVms();
  const navigate = useNavigate();

  return (
    <div>
      <PageHeader title="Virtual Machines" description="Manage VMs discovered from the daemon." />
      <Card>
        {isLoading && <p>Loading…</p>}
        {error && <p role="alert">{error.message}</p>}
        {!isLoading && !error && (vms?.length ?? 0) === 0 && (
          <EmptyState
            title="No VMs found"
            description="The daemon is not reporting any VMs yet. Create an appliance to launch one."
          />
        )}
        {(vms?.length ?? 0) > 0 && (
          <Table caption="Virtual machines">
            <thead>
              <tr><th>Name</th><th>State</th><th>Arch</th><th className="ifm-table__num">Memory</th><th className="ifm-table__nowrap">Actions</th></tr>
            </thead>
            <tbody>
              {vms?.map(vm => (
                <tr key={vm.id}>
                  <td>{vm.name}</td>
                  <td><StatusChip tone={vm.state === "running" ? "success" : "muted"} label={vm.state} /></td>
                  <td>{vm.arch}</td>
                  <td className="ifm-table__num">{vm.memory_mb} MB</td>
                  <td className="ifm-table__nowrap">
                    <Button
                      variant="ghost"
                      aria-label={`View ${vm.name}`}
                      onClick={() => navigate(`/vms/${vm.id}`)}
                    >
                      View
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        )}
      </Card>
    </div>
  );
}
