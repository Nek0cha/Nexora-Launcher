import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "react-router";
import "./styles/index.css";
import "./i18n";
import { Providers } from "./app/providers";
import { router } from "./app/router";
import { StartupGate } from "./app/StartupGate";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Providers>
      <StartupGate>
        <RouterProvider router={router} />
      </StartupGate>
    </Providers>
  </React.StrictMode>,
);
