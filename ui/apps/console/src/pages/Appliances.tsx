import React from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, PageHeader, StatusChip } from "@infrasim/ui";
import { Routes, Route, Link } from "react-router-dom";

export default function Appliances({ client }: { client: ReturnType<typeof createApiClient> }) {
  return (
    <Routes>
      <Route index element={<ApplianceList client={client} />} />
    </Routes>
  );
}

function ApplianceList({ client }: { client: ReturnType<typeof createApiClient> }) {
  const { data: appliances, isLoading, error } = client.hooks.useAppliances();
  const { data: templates } = client.hooks.useApplianceTemplates();
  const create = client.hooks.useCreateAppliance();

  return (
    <div>
      <PageHeader title="Appliances" description="Instances and templates." actions={<Link to="/">Dashboard</Link>} />
      <div className="grid">
        <Card title="Templates">
          <div className="template-grid">
            {templates?.map(t => (
              <div key={t.id} className="template-card">
                <div className="template-title">{t.title}</div>
                <p>{t.description}</p>
                <Button variant="primary" aria-label={`Create ${t.title}`} onClick={() => create.mutate({ name: `${t.id}-auto`, template_id: t.id, auto_start: true })}>Create</Button>
              </div>
            ))}
          </div>
        </Card>
        <Card title="Instances">
          {isLoading && <p>Loading…</p>}
          {error && <p role="alert">{error.message}</p>}
          {!appliances?.length && <p>No appliances yet.</p>}
          <ul className="appliance-list">
            {appliances?.map(a => (
              <li key={a.id}>
                <div className="row">
                  <div>
                    <strong>{a.name}</strong>
                    <div className="muted">Template: {a.template_id}</div>
                  </div>
                  <StatusChip tone={a.status === "running" ? "success" : "muted"} label={a.status} />
                  <Button variant="ghost" aria-label={`Boot ${a.name}`} onClick={() => client.hooks.useApplianceAction("boot").mutate({ id: a.id })}>Boot</Button>
                  <Button variant="ghost" aria-label={`Stop ${a.name}`} onClick={() => client.hooks.useApplianceAction("stop").mutate({ id: a.id, payload: { force: false } })}>Stop</Button>
                </div>
              </li>
            ))}
          </ul>
        </Card>
      </div>
    </div>
  );
}
