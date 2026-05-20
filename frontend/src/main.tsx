import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App.tsx";
import { HawkErrorBoundary } from "@/components/HawkErrorBoundary";
import { initHawk } from "@/lib/hawk";
import "./index.css";
import { initTheme } from "@/lib/theme";

initTheme();
initHawk();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <HawkErrorBoundary>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </HawkErrorBoundary>
  </StrictMode>,
);
