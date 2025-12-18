import { createApiClient } from "@infrasim/api-client";
import { useQuery } from "@tanstack/react-query";

export function useDaemonStatus(client: ReturnType<typeof createApiClient>) {
  return client.hooks.useDaemonStatus();
}
