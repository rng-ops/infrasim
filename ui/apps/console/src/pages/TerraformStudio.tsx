import React, { useState } from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, CodeBlock, PageHeader } from "@infrasim/ui";

export default function TerraformStudio({ client }: { client: ReturnType<typeof createApiClient> }) {
  const [applianceId, setApplianceId] = useState("");
  const { data, refetch, isFetching, error } = client.hooks.useTerraform(applianceId || "");

  return (
    <div>
      <PageHeader title="Terraform Studio" description="Generate HCL for appliances." />
      <Card>
        <label htmlFor="appliance-id">Appliance ID</label>
        <input id="appliance-id" value={applianceId} onChange={(e) => setApplianceId(e.target.value)} />
        <Button onClick={() => refetch()} disabled={!applianceId || isFetching} aria-label="Fetch terraform">Fetch</Button>
        {error && <p role="alert">{error.message}</p>}
        {data && <CodeBlock code={data.terraform_hcl} />}
      </Card>
    </div>
  );
}
