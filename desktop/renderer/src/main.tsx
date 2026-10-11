import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Panel } from "./Panel";
import "./styles.css";

// The same page is OpenAgora's main window and, with #panel, Sunday's panel.
const panel = window.location.hash === "#panel";
if (panel) document.body.classList.add("panel-window");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {panel ? <Panel /> : <App />}
  </StrictMode>,
);
