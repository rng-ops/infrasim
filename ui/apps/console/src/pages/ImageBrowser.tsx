/**
 * Docker Image Browser - Select container images and build appliances
 * 
 * Features:
 * - Browse local Docker/Podman images
 * - Search registries (Docker Hub, Quay, etc.)
 * - View image layers and history
 * - Add overlays (files, packages, commands)
 * - Configure network interfaces
 * - Generate qcow2 VM images from containers
 */

import React, { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Card, Button } from '@infrasim/ui';

// Types
interface ContainerImage {
  id: string;
  repository: string;
  tag: string;
  digest?: string;
  created: string;
  size: string;
  size_bytes: number;
  local: boolean;
  arch?: string;
  os?: string;
}

interface ImageLayer {
  id: string;
  created_by: string;
  size: string;
  size_bytes: number;
  created: string;
  comment: string;
}

interface NetworkInterface {
  id: string;
  name: string;
  type: 'user' | 'bridge' | 'vmnet_shared' | 'vmnet_bridged' | 'passthrough';
  mac_address?: string;
  ip_address?: string;
  gateway?: string;
  vlan_id?: number;
  mtu: number;
  bridge?: string;
  promiscuous: boolean;
}

interface ImageOverlay {
  id: string;
  name: string;
  type: 'files' | 'shell' | 'packages' | 'environment' | 'cloudinit';
  source_path?: string;
  dest_path?: string;
  commands: string[];
  packages: string[];
  env_vars: Record<string, string>;
}

interface RegistrySearchResult {
  name: string;
  description: string;
  stars: number;
  official: boolean;
  automated: boolean;
}

// API functions
const fetchDockerStatus = async () => {
  const res = await fetch('/api/docker/status');
  if (!res.ok) throw new Error('Failed to fetch Docker status');
  return res.json();
};

const fetchImages = async () => {
  const res = await fetch('/api/docker/images');
  if (!res.ok) throw new Error('Failed to fetch images');
  return res.json();
};

const fetchImageHistory = async (imageRef: string) => {
  const encoded = encodeURIComponent(imageRef);
  const res = await fetch(`/api/docker/images/${encoded}/history`);
  if (!res.ok) throw new Error('Failed to fetch image history');
  return res.json();
};

const searchRegistry = async (query: string) => {
  const res = await fetch(`/api/docker/search?q=${encodeURIComponent(query)}`);
  if (!res.ok) throw new Error('Search failed');
  return res.json();
};

const pullImage = async (image: string) => {
  const res = await fetch('/api/docker/images/pull', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ image }),
  });
  if (!res.ok) throw new Error('Pull failed');
  return res.json();
};

const buildAppliance = async (spec: any) => {
  const res = await fetch('/api/docker/build', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(spec),
  });
  if (!res.ok) throw new Error('Build failed');
  return res.json();
};

// Components
const ImageCard: React.FC<{
  image: ContainerImage;
  selected: boolean;
  onSelect: () => void;
}> = ({ image, selected, onSelect }) => {
  const fullRef = `${image.repository}:${image.tag}`;
  
  return (
    <div
      onClick={onSelect}
      className={`p-4 rounded-lg border cursor-pointer transition-all ${
        selected
          ? 'border-blue-500 bg-blue-500/10 ring-2 ring-blue-500/30'
          : 'border-zinc-700 bg-zinc-800/50 hover:border-zinc-500'
      }`}
    >
      <div className="flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <h3 className="font-medium text-white truncate">{image.repository}</h3>
          <p className="text-sm text-zinc-400">{image.tag}</p>
        </div>
        <span className="text-xs text-zinc-500 ml-2">{image.size}</span>
      </div>
      <div className="mt-2 flex items-center gap-2 text-xs text-zinc-500">
        <span className="px-1.5 py-0.5 bg-zinc-700 rounded">
          {image.id.slice(0, 12)}
        </span>
        <span>{image.created}</span>
      </div>
    </div>
  );
};

const LayerViewer: React.FC<{ imageRef: string }> = ({ imageRef }) => {
  const { data, isLoading, error } = useQuery({
    queryKey: ['image-history', imageRef],
    queryFn: () => fetchImageHistory(imageRef),
    enabled: !!imageRef,
  });

  if (isLoading) return <div className="text-zinc-400 p-4">Loading layers...</div>;
  if (error) return <div className="text-red-400 p-4">Failed to load layers</div>;

  const layers: ImageLayer[] = data?.layers || [];

  return (
    <div className="space-y-2">
      <h4 className="text-sm font-medium text-zinc-300">Image Layers ({layers.length})</h4>
      <div className="space-y-1 max-h-64 overflow-y-auto">
        {layers.map((layer, idx) => (
          <div
            key={layer.id || idx}
            className="p-2 bg-zinc-800/50 rounded text-xs font-mono"
          >
            <div className="flex items-center justify-between text-zinc-400">
              <span>{layer.size}</span>
              <span>{layer.created}</span>
            </div>
            <div className="text-zinc-300 truncate mt-1" title={layer.created_by}>
              {layer.created_by.slice(0, 100)}
              {layer.created_by.length > 100 && '...'}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const NetworkInterfaceEditor: React.FC<{
  interfaces: NetworkInterface[];
  onChange: (interfaces: NetworkInterface[]) => void;
}> = ({ interfaces, onChange }) => {
  const addInterface = () => {
    const newIface: NetworkInterface = {
      id: crypto.randomUUID(),
      name: `eth${interfaces.length}`,
      type: 'user',
      mtu: 1500,
      promiscuous: false,
    };
    onChange([...interfaces, newIface]);
  };

  const updateInterface = (id: string, updates: Partial<NetworkInterface>) => {
    onChange(interfaces.map(iface => 
      iface.id === id ? { ...iface, ...updates } : iface
    ));
  };

  const removeInterface = (id: string) => {
    onChange(interfaces.filter(iface => iface.id !== id));
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-zinc-300">Network Interfaces</h4>
        <button
          onClick={addInterface}
          className="text-xs px-2 py-1 bg-blue-600 hover:bg-blue-500 rounded text-white"
        >
          + Add Interface
        </button>
      </div>
      
      {interfaces.length === 0 && (
        <p className="text-xs text-zinc-500">No interfaces configured. A default NAT interface will be created.</p>
      )}

      {interfaces.map((iface) => (
        <div key={iface.id} className="p-3 bg-zinc-800/50 rounded-lg border border-zinc-700">
          <div className="flex items-center justify-between mb-2">
            <input
              type="text"
              value={iface.name}
              onChange={e => updateInterface(iface.id, { name: e.target.value })}
              className="bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-sm text-white w-24"
              placeholder="eth0"
            />
            <button
              onClick={() => removeInterface(iface.id)}
              className="text-red-400 hover:text-red-300 text-sm"
            >
              Remove
            </button>
          </div>
          
          <div className="grid grid-cols-2 gap-2 text-sm">
            <div>
              <label className="text-xs text-zinc-400">Type</label>
              <select
                value={iface.type}
                onChange={e => updateInterface(iface.id, { type: e.target.value as any })}
                className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-white"
              >
                <option value="user">NAT (User)</option>
                <option value="bridge">Bridge</option>
                <option value="vmnet_shared">VMNet Shared</option>
                <option value="vmnet_bridged">VMNet Bridged</option>
                <option value="passthrough">Passthrough</option>
              </select>
            </div>
            
            <div>
              <label className="text-xs text-zinc-400">MTU</label>
              <input
                type="number"
                value={iface.mtu}
                onChange={e => updateInterface(iface.id, { mtu: parseInt(e.target.value) || 1500 })}
                className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-white"
              />
            </div>

            {iface.type === 'bridge' && (
              <div className="col-span-2">
                <label className="text-xs text-zinc-400">Bridge Interface</label>
                <input
                  type="text"
                  value={iface.bridge || ''}
                  onChange={e => updateInterface(iface.id, { bridge: e.target.value })}
                  className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-white"
                  placeholder="br0"
                />
              </div>
            )}
          </div>
        </div>
      ))}
    </div>
  );
};

const OverlayEditor: React.FC<{
  overlays: ImageOverlay[];
  onChange: (overlays: ImageOverlay[]) => void;
}> = ({ overlays, onChange }) => {
  const addOverlay = (type: ImageOverlay['type']) => {
    const newOverlay: ImageOverlay = {
      id: crypto.randomUUID(),
      name: `overlay-${overlays.length + 1}`,
      type,
      commands: [],
      packages: [],
      env_vars: {},
    };
    onChange([...overlays, newOverlay]);
  };

  const updateOverlay = (id: string, updates: Partial<ImageOverlay>) => {
    onChange(overlays.map(o => o.id === id ? { ...o, ...updates } : o));
  };

  const removeOverlay = (id: string) => {
    onChange(overlays.filter(o => o.id !== id));
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-zinc-300">Overlays (Customizations)</h4>
        <div className="flex gap-1">
          <button
            onClick={() => addOverlay('shell')}
            className="text-xs px-2 py-1 bg-zinc-700 hover:bg-zinc-600 rounded text-white"
          >
            + Shell
          </button>
          <button
            onClick={() => addOverlay('packages')}
            className="text-xs px-2 py-1 bg-zinc-700 hover:bg-zinc-600 rounded text-white"
          >
            + Packages
          </button>
          <button
            onClick={() => addOverlay('files')}
            className="text-xs px-2 py-1 bg-zinc-700 hover:bg-zinc-600 rounded text-white"
          >
            + Files
          </button>
        </div>
      </div>

      {overlays.length === 0 && (
        <p className="text-xs text-zinc-500">No overlays. Image will be used as-is.</p>
      )}

      {overlays.map((overlay) => (
        <div key={overlay.id} className="p-3 bg-zinc-800/50 rounded-lg border border-zinc-700">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-2">
              <span className="text-xs px-1.5 py-0.5 bg-zinc-700 rounded uppercase">
                {overlay.type}
              </span>
              <input
                type="text"
                value={overlay.name}
                onChange={e => updateOverlay(overlay.id, { name: e.target.value })}
                className="bg-transparent border-none text-sm text-white focus:outline-none"
                placeholder="Overlay name"
              />
            </div>
            <button
              onClick={() => removeOverlay(overlay.id)}
              className="text-red-400 hover:text-red-300 text-sm"
            >
              Remove
            </button>
          </div>

          {overlay.type === 'shell' && (
            <div>
              <label className="text-xs text-zinc-400">Commands (one per line)</label>
              <textarea
                value={overlay.commands.join('\n')}
                onChange={e => updateOverlay(overlay.id, { commands: e.target.value.split('\n').filter(Boolean) })}
                className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-sm font-mono text-white h-24"
                placeholder="apt-get update&#10;apt-get install -y vim"
              />
            </div>
          )}

          {overlay.type === 'packages' && (
            <div>
              <label className="text-xs text-zinc-400">Packages (space-separated)</label>
              <input
                type="text"
                value={overlay.packages.join(' ')}
                onChange={e => updateOverlay(overlay.id, { packages: e.target.value.split(/\s+/).filter(Boolean) })}
                className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-sm text-white"
                placeholder="vim curl htop"
              />
            </div>
          )}

          {overlay.type === 'files' && (
            <div className="grid grid-cols-2 gap-2">
              <div>
                <label className="text-xs text-zinc-400">Source Path</label>
                <input
                  type="text"
                  value={overlay.source_path || ''}
                  onChange={e => updateOverlay(overlay.id, { source_path: e.target.value })}
                  className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-sm text-white"
                  placeholder="/path/to/files"
                />
              </div>
              <div>
                <label className="text-xs text-zinc-400">Dest Path</label>
                <input
                  type="text"
                  value={overlay.dest_path || ''}
                  onChange={e => updateOverlay(overlay.id, { dest_path: e.target.value })}
                  className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-sm text-white"
                  placeholder="/dest/in/image"
                />
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  );
};

// Main page component
export default function ImageBrowser() {
  const queryClient = useQueryClient();
  const [selectedImage, setSelectedImage] = useState<ContainerImage | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [showSearch, setShowSearch] = useState(false);
  const [applianceName, setApplianceName] = useState('');
  const [interfaces, setInterfaces] = useState<NetworkInterface[]>([]);
  const [overlays, setOverlays] = useState<ImageOverlay[]>([]);
  const [memoryMb, setMemoryMb] = useState(2048);
  const [cpuCores, setCpuCores] = useState(2);
  const [buildResult, setBuildResult] = useState<any>(null);

  // Queries
  const { data: status } = useQuery({
    queryKey: ['docker-status'],
    queryFn: fetchDockerStatus,
  });

  const { data: imagesData, isLoading: imagesLoading } = useQuery({
    queryKey: ['docker-images'],
    queryFn: fetchImages,
    enabled: status?.available,
  });

  const { data: searchResults, isLoading: searchLoading } = useQuery({
    queryKey: ['docker-search', searchQuery],
    queryFn: () => searchRegistry(searchQuery),
    enabled: showSearch && searchQuery.length > 2,
  });

  // Mutations
  const pullMutation = useMutation({
    mutationFn: pullImage,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['docker-images'] });
    },
  });

  const buildMutation = useMutation({
    mutationFn: buildAppliance,
    onSuccess: (data) => {
      setBuildResult(data);
    },
  });

  const images: ContainerImage[] = imagesData?.images || [];

  const handleBuild = () => {
    if (!selectedImage || !applianceName) return;
    
    const spec = {
      name: applianceName,
      base_image: `${selectedImage.repository}:${selectedImage.tag}`,
      arch: 'aarch64',
      memory_mb: memoryMb,
      cpu_cores: cpuCores,
      interfaces,
      overlays,
    };
    
    buildMutation.mutate(spec);
  };

  if (!status?.available) {
    return (
      <div className="p-6">
        <Card title="Docker Image Browser" className="bg-zinc-900 border-zinc-700">
          <div className="text-center py-12">
            <div className="text-6xl mb-4">🐳</div>
            <h2 className="text-xl font-semibold text-white mb-2">
              No Container Runtime Found
            </h2>
            <p className="text-zinc-400 max-w-md mx-auto">
              Install Docker or Podman to browse and build container images.
              The image browser requires a local container runtime.
            </p>
            <div className="mt-6 space-x-4">
              <a
                href="https://docs.docker.com/desktop/install/mac-install/"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded text-white"
              >
                Install Docker
              </a>
              <a
                href="https://podman.io/getting-started/installation"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-block px-4 py-2 bg-zinc-700 hover:bg-zinc-600 rounded text-white"
              >
                Install Podman
              </a>
            </div>
          </div>
        </Card>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white">Docker Image Browser</h1>
          <p className="text-zinc-400">
            Select a container image to build into a VM appliance
          </p>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs px-2 py-1 bg-green-900/50 text-green-400 rounded">
            {status?.runtime} available
          </span>
          <button
            onClick={() => setShowSearch(!showSearch)}
            className="px-3 py-1.5 bg-zinc-700 hover:bg-zinc-600 rounded text-white text-sm"
          >
            {showSearch ? 'Hide Search' : 'Search Registry'}
          </button>
        </div>
      </div>

      {/* Search panel */}
      {showSearch && (
        <Card className="bg-zinc-900 border-zinc-700">
          <div className="p-4">
            <div className="flex gap-2">
              <input
                type="text"
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                className="flex-1 bg-zinc-800 border border-zinc-600 rounded px-3 py-2 text-white"
                placeholder="Search Docker Hub..."
              />
            </div>
            
            {searchLoading && (
              <div className="text-zinc-400 text-sm mt-4">Searching...</div>
            )}
            
            {searchResults?.results && (
              <div className="mt-4 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
                {searchResults.results.slice(0, 12).map((result: RegistrySearchResult) => (
                  <div
                    key={result.name}
                    className="p-3 bg-zinc-800/50 rounded border border-zinc-700 hover:border-zinc-500"
                  >
                    <div className="flex items-center justify-between">
                      <h4 className="font-medium text-white truncate">{result.name}</h4>
                      <div className="flex items-center gap-1 text-xs text-zinc-400">
                        <span>⭐</span>
                        <span>{result.stars}</span>
                      </div>
                    </div>
                    <p className="text-xs text-zinc-400 mt-1 line-clamp-2">
                      {result.description || 'No description'}
                    </p>
                    <div className="mt-2 flex items-center gap-2">
                      {result.official && (
                        <span className="text-xs px-1 bg-blue-900/50 text-blue-400 rounded">
                          Official
                        </span>
                      )}
                      <button
                        onClick={() => pullMutation.mutate(result.name)}
                        disabled={pullMutation.isPending}
                        className="text-xs px-2 py-0.5 bg-green-600 hover:bg-green-500 rounded text-white ml-auto"
                      >
                        Pull
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </Card>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Image list */}
        <Card 
          title="Local Images" 
          actions={<span className="text-sm text-zinc-400">{images.length} images</span>}
          className="bg-zinc-900 border-zinc-700"
        >
          {imagesLoading ? (
            <div className="text-zinc-400 p-4">Loading images...</div>
          ) : images.length === 0 ? (
            <div className="text-zinc-400 p-4 text-center">
              No local images found. Pull an image from the registry.
            </div>
          ) : (
            <div className="grid gap-2 max-h-[500px] overflow-y-auto">
              {images.map((image) => (
                <ImageCard
                  key={image.id}
                  image={image}
                  selected={selectedImage?.id === image.id}
                  onSelect={() => setSelectedImage(image)}
                />
              ))}
            </div>
          )}
        </Card>

        {/* Selected image details */}
        <Card 
          title={selectedImage
            ? `${selectedImage.repository}:${selectedImage.tag}`
            : 'Select an Image'}
          className="bg-zinc-900 border-zinc-700"
        >
          <div className="space-y-4">
            {selectedImage ? (
              <>
                <div className="grid grid-cols-2 gap-4 text-sm">
                  <div>
                    <span className="text-zinc-400">Size:</span>
                    <span className="text-white ml-2">{selectedImage.size}</span>
                  </div>
                  <div>
                    <span className="text-zinc-400">Created:</span>
                    <span className="text-white ml-2">{selectedImage.created}</span>
                  </div>
                  <div className="col-span-2">
                    <span className="text-zinc-400">ID:</span>
                    <span className="text-white font-mono ml-2">{selectedImage.id}</span>
                  </div>
                </div>

                <LayerViewer imageRef={`${selectedImage.repository}:${selectedImage.tag}`} />
              </>
            ) : (
              <div className="text-zinc-400 text-center py-8">
                Select an image from the list to view details
              </div>
            )}
          </div>
        </Card>
      </div>

      {/* Appliance builder */}
      {selectedImage && (
        <Card 
          title="Build Appliance"
          className="bg-zinc-900 border-zinc-700"
        >
          <p className="text-sm text-zinc-400 mb-4">
            Configure and build a VM appliance from {selectedImage.repository}:{selectedImage.tag}
          </p>
          <div className="space-y-6">
            {/* Basic config */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Appliance Name</label>
                <input
                  type="text"
                  value={applianceName}
                  onChange={e => setApplianceName(e.target.value)}
                  className="w-full bg-zinc-800 border border-zinc-600 rounded px-3 py-2 text-white"
                  placeholder="my-appliance"
                />
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">Memory (MB)</label>
                <input
                  type="number"
                  value={memoryMb}
                  onChange={e => setMemoryMb(parseInt(e.target.value) || 2048)}
                  className="w-full bg-zinc-800 border border-zinc-600 rounded px-3 py-2 text-white"
                />
              </div>
              <div>
                <label className="block text-sm text-zinc-400 mb-1">CPU Cores</label>
                <input
                  type="number"
                  value={cpuCores}
                  onChange={e => setCpuCores(parseInt(e.target.value) || 2)}
                  className="w-full bg-zinc-800 border border-zinc-600 rounded px-3 py-2 text-white"
                />
              </div>
            </div>

            {/* Network interfaces */}
            <NetworkInterfaceEditor
              interfaces={interfaces}
              onChange={setInterfaces}
            />

            {/* Overlays */}
            <OverlayEditor
              overlays={overlays}
              onChange={setOverlays}
            />

            {/* Build button */}
            <div className="flex items-center justify-between pt-4 border-t border-zinc-700">
              <p className="text-sm text-zinc-400">
                This will generate a build plan. Review before executing.
              </p>
              <Button
                onClick={handleBuild}
                disabled={!applianceName || buildMutation.isPending}
              >
                {buildMutation.isPending ? 'Planning...' : 'Generate Build Plan'}
              </Button>
            </div>

            {/* Build result */}
            {buildResult && (
              <div className="mt-4 p-4 bg-zinc-800/50 rounded-lg border border-zinc-700">
                <h3 className="font-medium text-white mb-2">Build Plan Generated</h3>
                <div className="space-y-2">
                  <div>
                    <h4 className="text-sm text-zinc-400">Next Steps:</h4>
                    <ul className="list-disc list-inside text-sm text-zinc-300">
                      {buildResult.next_steps?.map((step: string, i: number) => (
                        <li key={i}>{step}</li>
                      ))}
                    </ul>
                  </div>
                  <details className="mt-2">
                    <summary className="text-sm text-blue-400 cursor-pointer">
                      View Terraform HCL
                    </summary>
                    <pre className="mt-2 p-3 bg-zinc-900 rounded text-xs font-mono text-zinc-300 overflow-x-auto">
                      {buildResult.terraform_hcl}
                    </pre>
                  </details>
                </div>
              </div>
            )}
          </div>
        </Card>
      )}
    </div>
  );
}
