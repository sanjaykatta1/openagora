// OpenAgora's icon on its own windows, on Linux, where each window carries
// its own icon for taskbars and window switchers. (Windows shows the
// program's icon and macOS the app's, both set when the app is packaged.)
// The packaged app keeps it in its resources; a development run uses the one
// in build/, where every OpenAgora icon is made (build/make-icons.py).

import { app } from "electron";
import fs from "node:fs";
import path from "node:path";

let found: string | null | undefined;

export function windowIcon(): string | undefined {
  if (process.platform !== "linux") return undefined;
  if (found === undefined) {
    const candidates = [
      path.join(process.resourcesPath ?? "", "icon.png"),
      path.join(app.getAppPath(), "build", "icons", "256x256.png"),
    ];
    found = candidates.find((file) => fs.existsSync(file)) ?? null;
  }
  return found ?? undefined;
}
