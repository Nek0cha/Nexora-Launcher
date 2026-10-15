export type PaletteCommand = { id: string; label: string; run: () => void };

export function filterCommands(commands: PaletteCommand[], query: string): PaletteCommand[] {
  const q = query.trim().toLowerCase();
  if (!q) return commands;
  return commands.filter((c) => c.label.toLowerCase().includes(q));
}

export function modifierLabel(platform: string): "⌘" | "Ctrl" {
  return /mac/i.test(platform) ? "⌘" : "Ctrl";
}

export function nextTheme(current: "dark" | "light"): "dark" | "light" {
  return current === "dark" ? "light" : "dark";
}
