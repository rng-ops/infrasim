import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";
import { execSync } from "child_process";

// Get git info for manifest
function getGitInfo() {
  try {
    return {
      commit: execSync("git rev-parse HEAD").toString().trim(),
      branch: execSync("git rev-parse --abbrev-ref HEAD").toString().trim(),
    };
  } catch {
    return { commit: "unknown", branch: "unknown" };
  }
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const isDev = mode === "development";
  
  // Rust backend address (for API proxy in dev mode)
  const backendUrl = env.INFRASIM_BACKEND_URL || "http://127.0.0.1:8080";
  
  // UI mount point - MUST match server routes and React Router basename
  const BASE_PATH = "/";
  
  const gitInfo = getGitInfo();

  return {
    plugins: [react()],
    
    // Base path for deployment under /ui/
    // This is a non-negotiable mount point invariant
    // Served by infrasim-web at the root in production.
    // Keep /ui/ working as a compatibility alias on the backend.
    base: '/',
    
    // Define global constants
    define: {
      __APP_VERSION__: JSON.stringify(process.env.npm_package_version || "0.0.0"),
      __GIT_COMMIT__: JSON.stringify(gitInfo.commit),
      __GIT_BRANCH__: JSON.stringify(gitInfo.branch),
      __BUILD_TIME__: JSON.stringify(new Date().toISOString()),
      __DEV_MODE__: JSON.stringify(isDev),
    },
    
    server: {
      port: 4173,
      strictPort: true,
      host: true,
      
      // Proxy configuration for development mode
      // In dev mode, Vite serves /ui/* with hot reload
      // API and websocket calls are proxied to Rust backend
      proxy: {
        // API endpoints
        "/api": {
          target: backendUrl,
          changeOrigin: true,
          secure: false,
        },
        // WebSocket proxy for VNC/console
        "/websockify": {
          target: backendUrl,
          changeOrigin: true,
          ws: true,
          secure: false,
        },
        // Admin endpoints
        "/admin": {
          target: backendUrl,
          changeOrigin: true,
          secure: false,
        },
        // noVNC static files (legacy)
        "/app": {
          target: backendUrl,
          changeOrigin: true,
        },
        "/core": {
          target: backendUrl,
          changeOrigin: true,
        },
        "/vendor": {
          target: backendUrl,
          changeOrigin: true,
        },
      },
      
      // HMR configuration
      hmr: {
        overlay: true,
      },
    },
    
    build: {
      outDir: "dist",
      sourcemap: !isDev ? "hidden" : true,
      
      // Deterministic builds for provenance
      rollupOptions: {
        output: {
          // Use content hashes for cache busting
          entryFileNames: "assets/[name]-[hash].js",
          chunkFileNames: "assets/[name]-[hash].js",
          assetFileNames: "assets/[name]-[hash].[ext]",
          
          // Manual chunk splitting for better caching
          manualChunks: {
            "vendor-react": ["react", "react-dom", "react-router-dom"],
            "vendor-query": ["@tanstack/react-query"],
            "vendor-ui": ["clsx"],
          },
        },
      },
      
      // Target modern browsers
      target: "es2020",
      
      // Minification
      minify: isDev ? false : "esbuild",
      
      // Report bundle size
      reportCompressedSize: true,
      
      // Chunk size warnings
      chunkSizeWarningLimit: 500,
    },
    
    // Resolve aliases
    resolve: {
      alias: {
        "@": resolve(__dirname, "src"),
      },
    },
    
    // CSS configuration
    css: {
      devSourcemap: true,
    },
    
    // Preview server (for testing production builds locally)
    preview: {
      port: 4174,
      strictPort: true,
      proxy: {
        "/api": {
          target: backendUrl,
          changeOrigin: true,
        },
        "/websockify": {
          target: backendUrl,
          changeOrigin: true,
          ws: true,
        },
      },
    },
    
    // Optimization
    optimizeDeps: {
      include: [
        "react",
        "react-dom",
        "react-router-dom",
        "@tanstack/react-query",
        "clsx",
      ],
    },
    
    // Environment variables prefix
    envPrefix: "INFRASIM_",
  };
});
