import React, { useState } from "react";
import { createApiClient } from "@infrasim/api-client";
import { Button, Card, PageHeader, CodeBlock } from "@infrasim/ui";

export default function AiBuilder({ client }: { client: ReturnType<typeof createApiClient> }) {
  const [prompt, setPrompt] = useState("");
  const ai = client.hooks.useAiDefine();
  return (
    <div>
      <PageHeader title="AI Builder" description="Generate appliance definitions from natural language." />
      <Card>
        <label htmlFor="prompt">Prompt</label>
        <textarea id="prompt" value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={5} style={{ width: "100%" }} />
        <Button onClick={() => ai.mutate({ prompt })} disabled={ai.isPending} aria-label="Generate from prompt">Generate</Button>
        {ai.data && <CodeBlock code={JSON.stringify(ai.data, null, 2)} />}
        {ai.error && <p role="alert">{ai.error.message}</p>}
      </Card>
    </div>
  );
}
