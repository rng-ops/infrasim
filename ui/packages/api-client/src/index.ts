import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { z } from "zod";
import {
  daemonStatusSchema,
  vmSchema,
  applianceInstanceSchema,
  applianceTemplateSchema,
  snapshotSchema,
  terraformSchema,
  aiDefineResponseSchema,
  evidenceResponseSchema,
  networkSchema,
  volumeSchema,
  attestationSchema,
  filesystemSchema,
  filesystemSnapshotSchema,
  resourceGraphSchema,
  graphPlanResultSchema,
  graphApplyResultSchema,
  graphValidationResultSchema,
  uiManifestSchema,
  type DaemonStatus,
  type Vm,
  type ApplianceInstance,
  type ApplianceTemplate,
  type Snapshot,
  type Terraform,
  type AiDefine,
  type Evidence,
  type Network,
  type Volume,
  type Attestation,
  type Filesystem,
  type FilesystemSnapshot,
  type ResourceGraph,
  type GraphPlanResult,
  type GraphApplyResult,
  type GraphValidationResult,
  type UiManifest,
} from "./schemas";

export { daemonStatusSchema } from "./schemas";
export type {
  DaemonStatus,
  Vm,
  ApplianceInstance,
  ApplianceTemplate,
  Snapshot,
  Terraform,
  AiDefine,
  Evidence,
  Network,
  Volume,
  Attestation,
  Filesystem,
  FilesystemSnapshot,
  FilesystemType,
  FilesystemLifecycle,
  GeographicBounds,
  ResourceGraph,
  ResourceNode,
  ResourceEdge,
  GraphPlanResult,
  GraphApplyResult,
  GraphValidationResult,
  UiManifest,
  UiManifestAsset,
} from "./schemas";

export type ApiError = { status: number; message: string; details?: unknown };

// SSE event types
export type SSEEvent = {
  type: string;
  data: unknown;
  id?: string;
  timestamp?: number;
};

export type SSEConnection = {
  close: () => void;
  isConnected: () => boolean;
};

export function createApiClient({
  baseUrl,
  getToken,
  onUnauthorized,
  devHeader,
}: {
  baseUrl: string;
  getToken: () => string | null;
  onUnauthorized?: () => void;
  devHeader?: boolean;
}) {
  const request = async <T>(path: string, schema: z.ZodType<T>, init?: RequestInit): Promise<T> => {
    const token = getToken();
    const res = await fetch(`${baseUrl}${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(devHeader ? { "x-infrasim-dev": "1" } : {}),
        ...(init?.headers as Record<string, string> | undefined),
      },
    });

    const text = await res.text();
    let json: unknown = {};
    if (text) {
      try {
        json = JSON.parse(text);
      } catch {
        json = { error: text };
      }
    }

    if (!res.ok) {
      const err: ApiError = {
        status: res.status,
        message: (json as { error?: string; message?: string })?.error || (json as { message?: string })?.message || res.statusText || "Request failed",
        details: json,
      };
      if (res.status === 401) onUnauthorized?.();
      throw err;
    }
    return schema.parse(json);
  };

  // SSE connection factory for real-time events
  const connectSSE = (path: string, onEvent: (event: SSEEvent) => void, onError?: (error: Error) => void): SSEConnection => {
    const token = getToken();
    const url = new URL(`${baseUrl}${path}`, window.location.origin);
    if (token) url.searchParams.set("token", token);
    
    let eventSource: EventSource | null = null;
    let connected = false;

    try {
      eventSource = new EventSource(url.toString());
      
      eventSource.onopen = () => {
        connected = true;
      };

      eventSource.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          onEvent({ type: "message", data, id: event.lastEventId, timestamp: Date.now() });
        } catch {
          onEvent({ type: "message", data: event.data, id: event.lastEventId, timestamp: Date.now() });
        }
      };

      eventSource.onerror = (e) => {
        connected = false;
        onError?.(new Error("SSE connection error"));
      };

      // Handle named events
      for (const eventType of ["appliance.created", "appliance.updated", "appliance.deleted", "vm.started", "vm.stopped", "snapshot.created", "graph.committed"]) {
        eventSource.addEventListener(eventType, (event) => {
          try {
            const data = JSON.parse((event as MessageEvent).data);
            onEvent({ type: eventType, data, id: (event as MessageEvent).lastEventId, timestamp: Date.now() });
          } catch {
            onEvent({ type: eventType, data: (event as MessageEvent).data, timestamp: Date.now() });
          }
        });
      }
    } catch (error) {
      onError?.(error instanceof Error ? error : new Error("Failed to connect SSE"));
    }

    return {
      close: () => {
        eventSource?.close();
        connected = false;
      },
      isConnected: () => connected,
    };
  };

  return {
    request,
    connectSSE,
    hooks: {
      useDaemonStatus: () => useQuery<DaemonStatus, ApiError>({
        queryKey: ["daemon-status"],
        queryFn: () => request("/api/daemon/status", daemonStatusSchema),
        refetchInterval: 15000,
      }),
      useVms: () => useQuery<Vm[], ApiError>({
        queryKey: ["vms"],
        queryFn: () => request("/api/vms", z.object({ vms: z.array(vmSchema) })).then((r) => r.vms),
        refetchInterval: 10000,
      }),
      useVm: (id: string) => useQuery<Vm, ApiError>({
        queryKey: ["vm", id],
        enabled: Boolean(id),
        queryFn: () => request(`/api/vms/${id}`, vmSchema),
        refetchInterval: 5000,
      }),
      useAppliances: () => useQuery<ApplianceInstance[], ApiError>({
        queryKey: ["appliances"],
        queryFn: () => request("/api/appliances", z.object({ appliances: z.array(applianceInstanceSchema) })).then((r) => r.appliances),
        refetchInterval: 10000,
      }),
      useApplianceDetail: (id: string) => useQuery<{
        instance: ApplianceInstance;
        template: ApplianceTemplate | null;
        vm: Vm | null;
        networks: Network[];
        volumes: Volume[];
        snapshots: Snapshot[];
        terraform_hcl: string;
        export_bundle: unknown;
      }, ApiError>({
        queryKey: ["appliance", id],
        enabled: Boolean(id),
        queryFn: () => request(`/api/appliances/${id}`, z.object({
          instance: applianceInstanceSchema,
          template: applianceTemplateSchema.nullable(),
          vm: vmSchema.nullable(),
          networks: z.array(networkSchema),
          volumes: z.array(volumeSchema),
          snapshots: z.array(snapshotSchema),
          terraform_hcl: z.string(),
          export_bundle: z.unknown(),
        })),
        refetchInterval: 10000,
      }),
      useApplianceTemplates: () => useQuery<ApplianceTemplate[], ApiError>({
        queryKey: ["appliance-templates"],
        queryFn: () => request("/api/appliances/templates", z.object({ templates: z.array(applianceTemplateSchema) })).then((r) => r.templates),
      }),
      useSnapshots: (vmId?: string) => useQuery<Snapshot[], ApiError>({
        queryKey: ["snapshots", vmId],
        queryFn: () => request(`/api/snapshots${vmId ? `?vm_id=${vmId}` : ""}`, z.object({ snapshots: z.array(snapshotSchema), count: z.number() })).then((r) => r.snapshots),
      }),
      useNetworks: () => useQuery<Network[], ApiError>({
        queryKey: ["networks"],
        queryFn: () => request("/api/networks", z.object({ networks: z.array(networkSchema), count: z.number() })).then((r) => r.networks),
        refetchInterval: 30000,
      }),
      useVolumes: () => useQuery<Volume[], ApiError>({
        queryKey: ["volumes"],
        queryFn: () => request("/api/volumes", z.object({ volumes: z.array(volumeSchema), count: z.number() })).then((r) => r.volumes),
        refetchInterval: 30000,
      }),
      useImages: () => useQuery<Volume[], ApiError>({
        queryKey: ["images"],
        queryFn: () => request("/api/images", z.object({ images: z.array(volumeSchema), count: z.number() })).then((r) => r.images),
        refetchInterval: 30000,
      }),
      useTerraform: (applianceId: string) => useQuery<Terraform, ApiError>({
        queryKey: ["terraform", applianceId],
        enabled: Boolean(applianceId),
        queryFn: () => request(`/api/appliances/${applianceId}/terraform`, terraformSchema),
      }),
      useAttestation: (applianceId: string) => useQuery<Attestation, ApiError>({
        queryKey: ["attestation", applianceId],
        enabled: Boolean(applianceId),
        queryFn: () => request(`/api/appliances/${applianceId}/attestation`, attestationSchema),
        staleTime: 60000,
      }),
      useAiDefine: () => useMutation<AiDefine, ApiError, { prompt: string }>({
        mutationFn: (vars) => request(`/api/ai/define`, aiDefineResponseSchema, { method: "POST", body: JSON.stringify(vars) }),
      }),
      useEvidence: () => useMutation<Evidence, ApiError, { appliance_id?: string; project_id?: string; purpose?: string }>({
        mutationFn: (vars) => request(`/api/provenance/evidence`, evidenceResponseSchema, { method: "POST", body: JSON.stringify(vars) }),
      }),
      useCreateAppliance: () => {
        const qc = useQueryClient();
        return useMutation<ApplianceInstance, ApiError, { name: string; template_id: string; auto_start?: boolean }>({
          mutationFn: (vars) => request(`/api/appliances`, applianceInstanceSchema, { method: "POST", body: JSON.stringify(vars) }),
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["vms"] });
          },
        });
      },
      useApplianceAction: (action: "boot" | "stop" | "snapshot" | "archive") => {
        const qc = useQueryClient();
        return useMutation<unknown, ApiError, { id: string; payload?: Record<string, unknown> }>({
          mutationFn: ({ id, payload }) => request(`/api/appliances/${id}/${action}`, z.unknown(), { method: "POST", body: JSON.stringify(payload || {}) }),
          onSuccess: (_d, vars) => {
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["appliance", vars.id] });
            qc.invalidateQueries({ queryKey: ["vms"] });
            qc.invalidateQueries({ queryKey: ["snapshots"] });
          },
        });
      },
      useBulkApplianceAction: (action: "boot" | "stop" | "archive") => {
        const qc = useQueryClient();
        return useMutation<unknown, ApiError, { ids: string[]; payload?: Record<string, unknown> }>({
          mutationFn: async ({ ids, payload }) => {
            const results = await Promise.allSettled(
              ids.map((id) => request(`/api/appliances/${id}/${action}`, z.unknown(), { method: "POST", body: JSON.stringify(payload || {}) }))
            );
            const errors = results.filter((r) => r.status === "rejected");
            if (errors.length > 0) {
              throw { status: 500, message: `${errors.length} of ${ids.length} operations failed`, details: errors };
            }
            return results;
          },
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["vms"] });
          },
        });
      },
      useExportAppliance: (applianceId: string) => useQuery<{ bundle: unknown }, ApiError>({
        queryKey: ["appliance-export", applianceId],
        enabled: false, // Manual trigger only
        queryFn: () => request(`/api/appliances/${applianceId}/export`, z.object({ bundle: z.unknown() })),
      }),
      useImportAppliance: () => {
        const qc = useQueryClient();
        return useMutation<ApplianceInstance, ApiError, { bundle: unknown; new_name?: string }>({
          mutationFn: (vars) => request(`/api/appliances/import`, applianceInstanceSchema, { method: "POST", body: JSON.stringify(vars) }),
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["appliances"] });
          },
        });
      },
      // Terraform plan/apply
      useTerraformGenerate: () => useMutation<{ terraform_hcl: string }, ApiError, { appliance_ids: string[] }>({
        mutationFn: (vars) => request(`/api/terraform/generate`, z.object({ terraform_hcl: z.string() }), { method: "POST", body: JSON.stringify(vars) }),
      }),
      useTerraformAudit: () => useMutation<{ valid: boolean; errors: string[]; warnings: string[] }, ApiError, { terraform_hcl: string }>({
        mutationFn: (vars) => request(`/api/terraform/audit`, z.object({ valid: z.boolean(), errors: z.array(z.string()), warnings: z.array(z.string()) }), { method: "POST", body: JSON.stringify(vars) }),
      }),

      // ========================================================================
      // UI Manifest (Provenance)
      // ========================================================================
      useUiManifest: () => useQuery<UiManifest, ApiError>({
        queryKey: ["ui-manifest"],
        queryFn: () => request("/api/ui/manifest", uiManifestSchema),
        staleTime: 60000 * 5, // 5 minutes
      }),

      // ========================================================================
      // Filesystem Resources (Terraform-addressable)
      // ========================================================================
      useFilesystems: () => useQuery<Filesystem[], ApiError>({
        queryKey: ["filesystems"],
        queryFn: () => request("/api/filesystems", z.array(filesystemSchema)),
        refetchInterval: 15000,
      }),
      useFilesystem: (id: string) => useQuery<Filesystem, ApiError>({
        queryKey: ["filesystem", id],
        enabled: Boolean(id),
        queryFn: () => request(`/api/filesystems/${id}`, filesystemSchema),
        refetchInterval: 10000,
      }),
      useCreateFilesystem: () => {
        const qc = useQueryClient();
        return useMutation<Filesystem, ApiError, Partial<Filesystem>>({
          mutationFn: (vars) => request(`/api/filesystems`, filesystemSchema, { method: "POST", body: JSON.stringify(vars) }),
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
          },
        });
      },
      useUpdateFilesystem: () => {
        const qc = useQueryClient();
        return useMutation<Filesystem, ApiError, { id: string; data: Partial<Filesystem> }>({
          mutationFn: ({ id, data }) => request(`/api/filesystems/${id}`, filesystemSchema, { method: "PUT", body: JSON.stringify(data) }),
          onSuccess: (_d, vars) => {
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["filesystem", vars.id] });
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
          },
        });
      },
      useDeleteFilesystem: () => {
        const qc = useQueryClient();
        return useMutation<void, ApiError, string>({
          mutationFn: (id) => request(`/api/filesystems/${id}`, z.unknown(), { method: "DELETE" }).then(() => undefined),
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
          },
        });
      },
      useFilesystemSnapshot: () => {
        const qc = useQueryClient();
        return useMutation<FilesystemSnapshot, ApiError, { id: string; name: string; description?: string }>({
          mutationFn: ({ id, ...data }) => request(`/api/filesystems/${id}/snapshot`, filesystemSnapshotSchema, { method: "POST", body: JSON.stringify(data) }),
          onSuccess: (_d, vars) => {
            qc.invalidateQueries({ queryKey: ["filesystem", vars.id] });
            qc.invalidateQueries({ queryKey: ["filesystems"] });
          },
        });
      },
      useAttachFilesystem: () => {
        const qc = useQueryClient();
        return useMutation<Filesystem, ApiError, { id: string; appliance_id: string; mount_point: string; read_only?: boolean }>({
          mutationFn: ({ id, ...data }) => request(`/api/filesystems/${id}/attach`, filesystemSchema, { method: "POST", body: JSON.stringify(data) }),
          onSuccess: (_d, vars) => {
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["filesystem", vars.id] });
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
          },
        });
      },
      useDetachFilesystem: () => {
        const qc = useQueryClient();
        return useMutation<Filesystem, ApiError, { id: string; appliance_id: string }>({
          mutationFn: ({ id, appliance_id }) => request(`/api/filesystems/${id}/detach/${appliance_id}`, filesystemSchema, { method: "POST" }),
          onSuccess: (_d, vars) => {
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["filesystem", vars.id] });
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
          },
        });
      },

      // ========================================================================
      // Resource Graph
      // ========================================================================
      useResourceGraph: () => useQuery<ResourceGraph, ApiError>({
        queryKey: ["resource-graph"],
        queryFn: () => request("/api/graph", resourceGraphSchema),
        refetchInterval: 10000,
      }),
      usePlanGraphChanges: () => useMutation<GraphPlanResult, ApiError, { operations: Array<{ op_type: string; resource_type: string; resource_id?: string; payload: unknown }> }>({
        mutationFn: (vars) => request(`/api/graph/plan`, graphPlanResultSchema, { method: "POST", body: JSON.stringify(vars) }),
      }),
      useApplyGraphChanges: () => {
        const qc = useQueryClient();
        return useMutation<GraphApplyResult, ApiError, { plan_id: string; force?: boolean }>({
          mutationFn: (vars) => request(`/api/graph/apply`, graphApplyResultSchema, { method: "POST", body: JSON.stringify(vars) }),
          onSuccess: () => {
            qc.invalidateQueries({ queryKey: ["resource-graph"] });
            qc.invalidateQueries({ queryKey: ["appliances"] });
            qc.invalidateQueries({ queryKey: ["filesystems"] });
            qc.invalidateQueries({ queryKey: ["networks"] });
            qc.invalidateQueries({ queryKey: ["volumes"] });
          },
        });
      },
      useValidateGraph: () => useQuery<GraphValidationResult, ApiError>({
        queryKey: ["graph-validation"],
        queryFn: () => request("/api/graph/validate", graphValidationResultSchema),
        staleTime: 30000,
      }),
    },
  };
}

export type ApiClient = ReturnType<typeof createApiClient>;
