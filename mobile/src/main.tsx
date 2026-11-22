import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import { App } from "./App";

// Android's WebView reports the status bar's height as a safe-area inset, but
// drops it to zero while the keyboard is open, just when the page is pushed
// up towards it. Remember the largest value seen and lay out against that.
const rememberInsets = () => {
  const probe = document.createElement("div");
  probe.style.cssText =
    "position:fixed;visibility:hidden;padding-top:env(safe-area-inset-top);padding-bottom:env(safe-area-inset-bottom)";
  document.body.appendChild(probe);
  const style = getComputedStyle(probe);
  const root = document.documentElement;
  for (const [edge, value] of [
    ["top", parseFloat(style.paddingTop)],
    ["bottom", parseFloat(style.paddingBottom)],
  ] as const) {
    const known = parseFloat(root.style.getPropertyValue(`--inset-${edge}`));
    if (value > (isNaN(known) ? 0 : known)) {
      root.style.setProperty(`--inset-${edge}`, `${value}px`);
    }
  }
  probe.remove();
};
rememberInsets();
window.addEventListener("resize", rememberInsets);
window.visualViewport?.addEventListener("resize", rememberInsets);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
