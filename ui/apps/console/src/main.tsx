import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { DesignSystemStyles } from "@infrasim/ui";
import "@infrasim/ui/dist/index.css";
import App from "./App";
import { StoreProvider } from "./store/store";
import "./global.css";

const qc = new QueryClient();

// Served by infrasim-web at the root in production.
// The backend still serves /ui/* as an alias for compatibility.
const BASE_PATH = "/";

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <QueryClientProvider client={qc}>
        <StoreProvider>
          <DesignSystemStyles />
          <BrowserRouter basename={BASE_PATH}>
            <App />
          </BrowserRouter>
        </StoreProvider>
      </QueryClientProvider>
    </React.StrictMode>
  );
}
