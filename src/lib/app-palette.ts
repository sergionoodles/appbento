export interface AppPalette {
  accent: string;
  onAccent: string;
}

const CHANNEL_MAX = 255;
const COLOR_BUCKET_SIZE = 32;
const MINIMUM_ALPHA = 32;
const SAMPLE_SIZE = 32;
const NEUTRAL_COLOR_WEIGHT = 0.01;

export function paletteFromName(name: string): AppPalette {
  const normalized = name.trim().toLowerCase().normalize("NFKC");
  const hash = Array.from(normalized).reduce(
    (value, character) => Math.imul(value ^ (character.codePointAt(0) ?? 0), 16777619) >>> 0,
    2166136261,
  );
  return { accent: `hsl(${hash % 360} 62% 64%)`, onAccent: "#14151f" };
}

function paletteFromRgb(red: number, green: number, blue: number): AppPalette {
  const channels = [red, green, blue].map((channel) => {
    const value = channel / CHANNEL_MAX;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  const luminance = channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  return {
    accent: `rgb(${red} ${green} ${blue})`,
    onAccent: luminance > 0.179 ? "#000000" : "#ffffff",
  };
}

/** Quantize similar colors so antialiasing does not split a logo's dominant color. */
export function paletteFromPixels(pixels: ArrayLike<number>): AppPalette | null {
  const buckets = new Map<string, { weight: number; red: number; green: number; blue: number }>();
  for (let index = 0; index + 3 < pixels.length; index += 4) {
    const [red, green, blue, alpha] = [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]];
    if (alpha < MINIMUM_ALPHA) continue;
    const saturation = (Math.max(red, green, blue) - Math.min(red, green, blue)) / CHANNEL_MAX;
    // Prefer brand colors over white/black padding, while retaining monochrome logos.
    const weight = (alpha / CHANNEL_MAX) * (NEUTRAL_COLOR_WEIGHT + saturation ** 2);
    const key = [red, green, blue].map((channel) => Math.floor(channel / COLOR_BUCKET_SIZE)).join(",");
    const bucket = buckets.get(key) ?? { weight: 0, red: 0, green: 0, blue: 0 };
    buckets.set(key, {
      weight: bucket.weight + weight,
      red: bucket.red + red * weight,
      green: bucket.green + green * weight,
      blue: bucket.blue + blue * weight,
    });
  }
  const dominant = Array.from(buckets.values()).sort((first, second) => second.weight - first.weight)[0];
  if (!dominant) return null;
  return paletteFromRgb(
    Math.round(dominant.red / dominant.weight),
    Math.round(dominant.green / dominant.weight),
    Math.round(dominant.blue / dominant.weight),
  );
}

export function extractImagePalette(source: string): Promise<AppPalette | null> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        const scale = SAMPLE_SIZE / Math.max(image.naturalWidth, image.naturalHeight);
        canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
        canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
        const context = canvas.getContext("2d", { willReadFrequently: true });
        if (!context) throw new Error("Image color extraction is unavailable.");
        context.drawImage(image, 0, 0, canvas.width, canvas.height);
        resolve(paletteFromPixels(context.getImageData(0, 0, canvas.width, canvas.height).data));
      } catch (error) {
        reject(error);
      }
    };
    image.onerror = () => reject(new Error("Icon could not be read for color extraction."));
    image.src = source;
  });
}
