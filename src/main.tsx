/**
 * Entry point and surface router.
 *
 * Every OVERLAY window — the studio, each widget, the dock, the HUD, the
 * control centre, the dropzone — loads this same bundle. The `?surface=`
 * query the Rust window manager puts in the URL decides which tree mounts.
 *
 * One bundle rather than several keeps the theme engine, the design system
 * and the API layer identical across every surface, which is why a widget
 * looks like it belongs to the same product as the studio.
 */

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@/themes/theme.css";

import { Studio } from "@/app/Studio";
import { ControlCenterSurface } from "@/surfaces/ControlCenterSurface";
import { DockSurface } from "@/surfaces/DockSurface";
import { DropzoneSurface } from "@/surfaces/DropzoneSurface";
import { HudSurface } from "@/surfaces/HudSurface";
import { WidgetSurface } from "@/surfaces/WidgetSurface";
import { ToastProvider } from "@/components/ui";

const params = new URLSearchParams(window.location.search);
const surface = params.get("surface") ?? "studio";

/** Surfaces are transparent, frameless windows; the studio is a real window. */
if (surface !== "studio") {
  document.body.dataset.surface = surface;
}

function Root() {
  switch (surface) {
    case "widget":
      return <WidgetSurface widgetId={params.get("id") ?? "clock"} />;
    case "dock":
      return <DockSurface />;
    case "hud":
      return <HudSurface />;
    case "control-center":
      return <ControlCenterSurface />;
    case "dropzone":
      return <DropzoneSurface />;
    default:
      return (
        <ToastProvider>
          <Studio />
        </ToastProvider>
      );
  }
}

const container = document.getElementById("root");
if (!container) {
  throw new Error("#root is missing from index.html");
}

createRoot(container).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
