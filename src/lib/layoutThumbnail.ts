type ThumbnailItem = {
  id: string;
  kind?: "visual" | "text" | "image" | "shape";
  package_id?: string | null;
  export_name?: string | null;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  z_index: number;
  visible: boolean;
  opacity: number;
  settings: Record<string, unknown>;
};

type ThumbnailLayer = {
  kind?: "normal" | "event";
  visible: boolean;
  order: number;
  items: ThumbnailItem[];
};

type ThumbnailBackground = {
  kind: "color" | "image";
  color: string;
  image?: string | null;
  fit?: "cover" | "contain" | "stretch";
};

export type ThumbnailLayout = {
  name: string;
  width: number;
  height: number;
  layers: ThumbnailLayer[];
  items?: ThumbnailItem[];
  background?: ThumbnailBackground;
};

type ThumbnailOptions = {
  kind: "overlay" | "page";
};

const THUMBNAIL_WIDTH = 640;

export function createLayoutThumbnail(layout: ThumbnailLayout, options: ThumbnailOptions) {
  const width = Math.max(320, Math.round(Number(layout.width) || 16));
  const height = Math.max(240, Math.round(Number(layout.height) || 9));
  const outputHeight = Math.max(180, Math.round((THUMBNAIL_WIDTH / width) * height));
  const background = layout.background;
  const layers = layoutLayers(layout);
  const parts: string[] = [];

  parts.push(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${THUMBNAIL_WIDTH}" height="${outputHeight}" viewBox="0 0 ${width} ${height}">`
  );
  parts.push(`<rect width="${width}" height="${height}" fill="${escapeAttr(backgroundColor(background, options.kind))}"/>`);
  if (background?.kind === "image" && background.image) {
    parts.push(`<image href="${escapeAttr(background.image)}" x="0" y="0" width="${width}" height="${height}" preserveAspectRatio="${preserveAspect(background.fit)}" opacity="0.72"/>`);
  }

  for (const layer of layers) {
    if (layer.visible === false) continue;
    const items = [...(layer.items ?? [])]
      .filter((item) => item.visible !== false)
      .sort((a, b) => a.z_index - b.z_index);
    for (const item of items) {
      parts.push(renderItem(item));
    }
  }

  parts.push("</svg>");
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(parts.join(""))}`;
}

function layoutLayers(layout: ThumbnailLayout) {
  const layers = layout.layers?.length
    ? layout.layers
    : [
        {
          kind: "normal" as const,
          visible: true,
          order: 0,
          items: layout.items ?? []
        }
      ];
  return [...layers].sort((a, b) => {
    if (a.kind === "event" && b.kind !== "event") return 1;
    if (a.kind !== "event" && b.kind === "event") return -1;
    return a.order - b.order;
  });
}

function renderItem(item: ThumbnailItem) {
  const opacity = clamp(Number(item.opacity ?? 1), 0, 1);
  const x = Math.round(item.x);
  const y = Math.round(item.y);
  const width = Math.max(1, Math.round(item.width));
  const height = Math.max(1, Math.round(item.height));
  const radius = Math.min(16, Math.max(3, Math.round(Math.min(width, height) * 0.05)));
  const kind = item.kind ?? "visual";

  if (kind === "text") {
    const text = String(item.settings?.text ?? item.name ?? "");
    const color = safePaint(item.settings?.color, "#f8fafc");
    const fontSize = clamp(Number(item.settings?.fontSize ?? 24), 8, 96);
    return [
      `<g opacity="${opacity}">`,
      `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="rgba(15,23,42,0.16)"/>`,
      `<text x="${x + width / 2}" y="${y + height / 2}" dominant-baseline="middle" text-anchor="middle" fill="${escapeAttr(color)}" font-family="Inter, system-ui, sans-serif" font-size="${fontSize}" font-weight="700">${escapeText(truncate(text, 42))}</text>`,
      `</g>`
    ].join("");
  }

  if (kind === "shape") {
    const fill = safePaint(item.settings?.fill, "rgba(56,189,248,0.24)");
    const stroke = safePaint(item.settings?.borderColor, "rgba(248,250,252,0.38)");
    const strokeWidth = clamp(Number(item.settings?.borderWidth ?? 1), 0, 12);
    return `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${escapeAttr(fill)}" stroke="${escapeAttr(stroke)}" stroke-width="${strokeWidth}" opacity="${opacity}"/>`;
  }

  if (kind === "image") {
    const src = typeof item.settings?.src === "string" ? item.settings.src : "";
    if (src) {
      return [
        `<g opacity="${opacity}">`,
        `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="rgba(15,23,42,0.42)"/>`,
        `<image href="${escapeAttr(src)}" x="${x}" y="${y}" width="${width}" height="${height}" preserveAspectRatio="xMidYMid slice"/>`,
        `</g>`
      ].join("");
    }
  }

  const label = item.name || item.export_name || "Visual";
  return [
    `<g opacity="${opacity}">`,
    `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="rgba(14,165,233,0.16)" stroke="rgba(125,211,252,0.7)" stroke-width="2"/>`,
    `<text x="${x + width / 2}" y="${y + height / 2}" dominant-baseline="middle" text-anchor="middle" fill="#e0f2fe" font-family="Inter, system-ui, sans-serif" font-size="${Math.max(12, Math.min(28, height * 0.18))}" font-weight="800">${escapeText(truncate(label, 34))}</text>`,
    `</g>`
  ].join("");
}

function backgroundColor(background: ThumbnailBackground | undefined, kind: ThumbnailOptions["kind"]) {
  if (background?.kind === "color" || background?.color) return safePaint(background.color, "#0f172a");
  return kind === "overlay" ? "#10151f" : "#0f172a";
}

function preserveAspect(fit: ThumbnailBackground["fit"] = "cover") {
  if (fit === "stretch") return "none";
  if (fit === "contain") return "xMidYMid meet";
  return "xMidYMid slice";
}

function safePaint(value: unknown, fallback: string) {
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > 120) return fallback;
  return /^[#a-zA-Z0-9(),.%\s-]+$/.test(trimmed) ? trimmed : fallback;
}

function escapeAttr(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/"/g, "&quot;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function escapeText(value: string) {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function truncate(value: string, length: number) {
  return value.length > length ? `${value.slice(0, Math.max(0, length - 3))}...` : value;
}

function clamp(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}
