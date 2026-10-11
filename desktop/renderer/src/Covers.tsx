// App tabs are drawn by OpenAgora's main process over this page, so a dialog
// on this page would be hidden under an app's tab. While one is open, the
// tabs step aside.
import { useEffect } from "react";
import { bridge } from "./api";

let open = 0;

/** Render inside any dialog that may show over an app's tab. */
export function CoversTabs(): null {
  useEffect(() => {
    open += 1;
    if (open === 1) bridge?.tabs?.cover(true);
    return () => {
      open -= 1;
      if (open === 0) bridge?.tabs?.cover(false);
    };
  }, []);
  return null;
}
