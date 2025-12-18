import React, { useState } from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, PageHeader, CodeBlock } from "@infrasim/ui";

export default function Evidence({ client }: { client: ReturnType<typeof createApiClient> }) {
  const [applianceId, setApplianceId] = useState("");
  const [purpose, setPurpose] = useState("snapshot");
  const evidence = client.hooks.useEvidence();
  return (
    <div>
      <PageHeader title="Evidence" description="Generate provenance evidence bundles." />
      <Card>
        <label htmlFor="appliance">Appliance ID</label>
        <input id="appliance" value={applianceId} onChange={(e) => setApplianceId(e.target.value)} />
        <label htmlFor="purpose">Purpose</label>
        <input id="purpose" value={purpose} onChange={(e) => setPurpose(e.target.value)} />
        <Button onClick={() => evidence.mutate({ appliance_id: applianceId, purpose })} aria-label="Generate evidence">Generate</Button>
        {evidence.data && <CodeBlock code={JSON.stringify(evidence.data, null, 2)} />}
        {evidence.error && <p role="alert">{evidence.error.message}</p>}
      </Card>
    </div>
  );
}
