import React from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, PageHeader } from "@infrasim/ui";

export default function Admin({ client }: { client: ReturnType<typeof createApiClient> }) {
  const restartWeb = () => fetch("/api/admin/restart-web", { method: "POST" });
  const restartDaemon = () => fetch("/api/admin/restart-daemon", { method: "POST" });
  const stopDaemon = () => fetch("/api/admin/stop-daemon", { method: "POST" });
  return (
    <div>
      <PageHeader title="Admin" description="Dangerous actions. Dev use only." />
      <Card>
        <Button variant="danger" onClick={restartWeb} aria-label="Restart web">Restart Web</Button>
        <Button variant="danger" onClick={restartDaemon} aria-label="Restart daemon">Restart Daemon</Button>
        <Button variant="danger" onClick={stopDaemon} aria-label="Stop daemon">Stop Daemon</Button>
      </Card>
    </div>
  );
}
