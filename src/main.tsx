import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// WebKitGTK's default right-click menu (Back/Forward/Reload/Inspect Element)
// leaks browser chrome into what should look like a native app - suppressed
// in production builds only, so Inspect Element still works during dev.
if (import.meta.env.PROD) {
  document.addEventListener("contextmenu", (e) => e.preventDefault());
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
