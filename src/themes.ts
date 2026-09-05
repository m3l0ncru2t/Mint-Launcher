import familiaImage from "./assets/familia.jpg";

export interface ThemeOpacityDefaults {
  sidebar: number;
  modsPanel: number;
}

export interface BackgroundTheme {
  id: string;
  label: string;
  /** CSS `background-image` value - "none" for the plain default look. */
  css: string;
  /** Sidebar/addon-list transparency this theme starts at until the player
   * saves their own (see `Settings.themeOpacity`). */
  defaultOpacity: ThemeOpacityDefaults;
}

const STANDARD_OPACITY: ThemeOpacityDefaults = { sidebar: 0.82, modsPanel: 0.82 };

/** Preset ids/gradients live entirely on the frontend - the backend only
 * ever stores the chosen theme id as a plain string, so adding/renaming
 * presets never touches settings.rs. Presets are shipped with the app
 * (bundled as build assets where they use an image) and can't be removed
 * from the gallery, unlike a player's own uploaded custom backgrounds. */
export const BACKGROUND_THEMES: BackgroundTheme[] = [
  { id: "default", label: "Default", css: "none", defaultOpacity: STANDARD_OPACITY },
  {
    id: "sunset",
    label: "Sunset",
    css: "linear-gradient(160deg, #2b1a12 0%, #7c3a24 45%, #d97b3f 100%)",
    defaultOpacity: STANDARD_OPACITY,
  },
  {
    id: "forest",
    label: "Forest",
    css: "linear-gradient(160deg, #0d1f16 0%, #14361f 50%, #1f5c33 100%)",
    defaultOpacity: STANDARD_OPACITY,
  },
  {
    id: "midnight",
    label: "Midnight",
    css: "linear-gradient(160deg, #05070f 0%, #10162b 50%, #1c2b4a 100%)",
    defaultOpacity: STANDARD_OPACITY,
  },
  {
    id: "slate",
    label: "Slate",
    css: "linear-gradient(160deg, #14181a 0%, #232c30 55%, #35434a 100%)",
    defaultOpacity: STANDARD_OPACITY,
  },
  {
    id: "familia",
    label: "FAMILIA!!!",
    css: `url("${familiaImage}")`,
    defaultOpacity: { sidebar: 0.0, modsPanel: 0.82 },
  },
];
