export interface ThemeDef {
  id: string;
  label: string;
  blurb: string;
}

export const THEMES: ThemeDef[] = [
  { id: "tokyo-night", label: "Tokyo Night", blurb: "neon night // default" },
  { id: "gruvbox", label: "Gruvbox", blurb: "retro groove" },
  { id: "catppuccin", label: "Catppuccin", blurb: "pastel mocha" },
  { id: "nord", label: "Nord", blurb: "arctic calm" },
  { id: "dracula", label: "Dracula", blurb: "purple hiss" },
  { id: "terminal", label: "Terminal", blurb: "phosphor green" },
  { id: "paper", label: "Paper", blurb: "light mode" },
];
