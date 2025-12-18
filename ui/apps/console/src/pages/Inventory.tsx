import React from "react";
import { createApiClient } from "@infrasim/api-client";
import { Card, PageHeader } from "@infrasim/ui";

export default function Inventory({ client }: { client: ReturnType<typeof createApiClient> }) {
  const volumes = client.hooks.useVms(); // placeholder; should wire proper inventory endpoints in full build
  return (
    <div>
      <PageHeader title="Inventory" description="Images, volumes, networks, snapshots." />
      <Card>
        <p>Inventory views will list images, volumes, networks, and snapshots with filters.</p>
        <p>Placeholder count: {volumes.data?.length ?? 0}</p>
      </Card>
    </div>
  );
}
