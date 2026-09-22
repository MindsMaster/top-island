export interface ShellCommands {
  reveal(): void;
  openPanel(panelId: string): void;
}

let impl: ShellCommands | null = null;

export function setShellCommands(c: ShellCommands) {
  impl = c;
}

export const shell: ShellCommands = {
  reveal: () => impl?.reveal(),
  openPanel: (id) => impl?.openPanel(id),
};
