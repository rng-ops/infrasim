import React from "react";
import { useParams } from "react-router-dom";
import { createApiClient } from "@infrasim/api-client";
import { Card, PageHeader, StatusChip, Tabs, Table, EmptyState } from "@infrasim/ui";
import { useSelector } from "../store/store";

export default function VmDetail({ client }: { client: ReturnType<typeof createApiClient> }) {
  const { id } = useParams();
  const isVisible = useSelector(s => s.ui.isPageVisible);

  const { data: vms, isLoading, error } = client.hooks.useVms();
  const vm = vms?.find(v => v.id === id);
  const { data: snaps } = client.hooks.useSnapshots(id);

  return (
    <div>
      <PageHeader
        title={vm ? vm.name : "VM"}
        description={id ? `VM id: ${id}` : ""}
        actions={vm ? <StatusChip tone={vm.state === "running" ? "success" : "muted"} label={vm.state} /> : undefined}
      />

      {isLoading && <Card><p>Loading…</p></Card>}
      {error && <Card><p role="alert">{error.message}</p></Card>}
      {!isLoading && !error && !vm && <Card><EmptyState title="VM not found" description="This VM id is not currently in the VM list." /></Card>}

      {vm && (
        <Card>
          <Tabs
            items={[
              {
                id: "overview",
                label: "Overview",
                panel: (
                  <div>
                    <dl>
                      <dt>Architecture</dt>
                      <dd>{vm.arch}</dd>
                      <dt>Memory</dt>
                      <dd>{vm.memory_mb} MB</dd>
                    </dl>
                    {!isVisible && <p className="sr-only">Polling paused while page hidden.</p>}
                  </div>
                ),
              },
              {
                id: "snapshots",
                label: "Snapshots",
                panel: (
                  <div>
                    {(snaps?.length ?? 0) === 0 ? (
                      <EmptyState title="No snapshots" description="Create a snapshot from an appliance to see it here." />
                    ) : (
                      <Table caption="Snapshots">
                        <thead>
                          <tr><th>ID</th><th>Created</th><th>Has memory</th></tr>
                        </thead>
                        <tbody>
                          {snaps?.map(s => (
                            <tr key={s.id}>
                              <td>{s.id}</td>
                              <td>{new Date(s.created_at).toLocaleString()}</td>
                              <td><StatusChip tone={s.memory_path ? "success" : "muted"} label={s.memory_path ? "yes" : "no"} /></td>
                            </tr>
                          ))}
                        </tbody>
                      </Table>
                    )}
                  </div>
                ),
              },
            ]}
          />
        </Card>
      )}
    </div>
  );
}
