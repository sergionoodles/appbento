export interface ThemeDef {
  id: string;
  label: string;
  blurb: string;
}

export const THEMES: ThemeDef[] = [
  { id: "midnight", label: "Midnight", blurb: "ink and lilac" },
  { id: "daylight", label: "Daylight", blurb: "cool light" },
  { id: "tokyo-night", label: "Tokyo Night", blurb: "neon night" },
  { id: "gruvbox", label: "Gruvbox", blurb: "retro groove" },
  { id: "catppuccin", label: "Catppuccin", blurb: "pastel mocha" },
  { id: "nord", label: "Nord", blurb: "arctic calm" },
  { id: "dracula", label: "Dracula", blurb: "purple hiss" },
  { id: "terminal", label: "Terminal", blurb: "phosphor green" },
  { id: "paper", label: "Paper", blurb: "warm light" },
];
