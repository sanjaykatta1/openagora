import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { agentCommand, agentWorkflow } from "./agent-workflows";

export function commandDirectories(env: NodeJS.ProcessEnv = process.env, platform = process.platform, home = os.homedir()): string[] {
  const extra = platform === "win32"
    ? [env.LOCALAPPDATA && path.join(env.LOCALAPPDATA, "hermes", "bin"), env.APPDATA && path.join(env.APPDATA, "npm")]
    : ["/opt/homebrew/bin", "/usr/local/bin"];
  return [...extra, path.join(home, ".local", "bin"), path.join(home, ".cargo", "bin"),
    ...(env.PATH ?? "").split(path.delimiter)].filter((p): p is string => !!p && path.isAbsolute(p));
}

export function findCommand(command: string, directories = commandDirectories(), platform = process.platform): string | undefined {
  const extensions = platform === "win32" ? [".exe", ".cmd", ".bat", ""] : [""];
  for (const dir of directories) for (const extension of extensions) {
    const candidate = path.join(dir, command + extension);
    try {
      if (!fs.statSync(candidate).isFile()) continue;
      fs.accessSync(candidate, platform === "win32" ? fs.constants.F_OK : fs.constants.X_OK);
      return candidate;
    } catch { /* Not installed here. */ }
  }
}

export function agentAvailability(id: unknown): { detected: boolean; launchable: boolean } {
  const workflow = agentWorkflow(id);
  return { detected: !!workflow.executable && !!findCommand(workflow.executable), launchable: !!workflow.executable };
}

export const shQuote = (text: string) => `'${text.replaceAll("'", "'\\''")}'`;
export const psQuote = (text: string) => `'${text.replaceAll("'", "''")}'`;

export function nativeCommand(executable: string, args: string[], directory: string, platform = process.platform): string {
  if (platform === "win32") {
    return `Set-Location -LiteralPath ${psQuote(directory)}; & ${[executable, ...args].map(psQuote).join(" ")}`;
  }
  return `cd ${shQuote(directory)} || exit 1\n${[executable, ...args].map(shQuote).join(" ")}\n`;
}

function launch(command: string, args: string[], env: NodeJS.ProcessEnv): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, detached: true, stdio: "ignore", windowsHide: true });
    child.once("error", reject);
    child.once("spawn", () => { child.unref(); resolve(); });
  });
}

/** Open an interactive terminal. No prompts, credentials, or agent output are captured. */
export async function launchAgent(id: unknown, action: unknown, userData: string): Promise<void> {
  const selected = agentCommand(id, action);
  const workflow = agentWorkflow(id);
  const executable = workflow.executable && findCommand(workflow.executable);
  if (!executable) throw new Error("The agent’s CLI was not found. Install it using its official guide, then refresh. Desktop-only installs may not provide a CLI.");
  const directory = path.join(userData, "agent-workspaces", id as string);
  fs.mkdirSync(directory, { recursive: true });
  const env = { ...process.env, PATH: commandDirectories().join(path.delimiter) };
  const command = nativeCommand(executable, selected.args, directory);
  if (process.platform === "win32") {
    const encoded = Buffer.from(command, "utf16le").toString("base64");
    const powershell = path.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
    // EncodedCommand avoids cmd.exe expansion and handles spaces/quotes in user paths.
    await launch(powershell, ["-NoProfile", "-NonInteractive", "-Command",
      `Start-Process ${psQuote(powershell)} -ArgumentList '-NoLogo','-NoProfile','-NoExit','-EncodedCommand','${encoded}'`], env);
    return;
  }
  const terminal = process.platform === "darwin" ? "/usr/bin/open"
    : ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "xterm"].map((name) => findCommand(name)).find(Boolean);
  if (!terminal) throw new Error("No terminal app was found. Use the command shown below in your preferred terminal.");
  const sessions = path.join(userData, "agent-launchers");
  fs.mkdirSync(sessions, { recursive: true, mode: 0o700 });
  const script = path.join(sessions, `${randomUUID()}.command`);
  // Delete the launcher as soon as it starts. It contains only paths and fixed commands.
  fs.writeFileSync(script, ["#!/bin/sh", 'rm -f -- "$0"', `export PATH=${shQuote(env.PATH)}`, command,
    "printf '\\nAgent command finished. Press Enter to close. '", "read answer", ""].join("\n"), { mode: 0o700 });
  const name = path.basename(terminal);
  const args = process.platform === "darwin" ? ["-a", "Terminal", script]
    : name === "gnome-terminal" ? ["--", "/bin/sh", script]
    : name === "xfce4-terminal" ? ["--execute", "/bin/sh", script]
    : ["-e", "/bin/sh", script];
  try { await launch(terminal, args, env); }
  catch (error) { fs.rmSync(script, { force: true }); throw error; }
}
