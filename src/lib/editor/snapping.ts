export type SnapCanvas = {
  width: number;
  height: number;
};

export type SnapItem = {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
};

export type SnapGuides = {
  x: number | null;
  y: number | null;
};

type SnapCandidate = {
  position: number;
  guide: number;
};

type SnapOptions = {
  enabled: boolean;
  gridSize: number;
  threshold?: number;
};

const DEFAULT_SNAP_THRESHOLD = 8;

export function emptySnapGuides(): SnapGuides {
  return { x: null, y: null };
}

export function snapItemPosition(
  item: SnapItem,
  canvas: SnapCanvas,
  items: SnapItem[],
  options: SnapOptions
): SnapGuides {
  if (!options.enabled) return emptySnapGuides();

  const xSnap = snapAxis(item.x, xCandidates(item, canvas, items), options);
  const ySnap = snapAxis(item.y, yCandidates(item, canvas, items), options);
  item.x = xSnap.value;
  item.y = ySnap.value;
  return { x: xSnap.guide, y: ySnap.guide };
}

export function snapItemResize(
  item: SnapItem,
  canvas: SnapCanvas,
  items: SnapItem[],
  options: SnapOptions
): SnapGuides {
  if (!options.enabled) return emptySnapGuides();

  const rightSnap = snapAxis(item.x + item.width, xResizeCandidates(item, canvas, items), options);
  const bottomSnap = snapAxis(item.y + item.height, yResizeCandidates(item, canvas, items), options);
  item.width = rightSnap.value - item.x;
  item.height = bottomSnap.value - item.y;
  return { x: rightSnap.guide, y: bottomSnap.guide };
}

export function snapGuideStyle(axis: "x" | "y", value: number | null, canvas: SnapCanvas | null) {
  if (!canvas || value === null) return "";
  return axis === "x" ? `left:${(value / canvas.width) * 100}%;` : `top:${(value / canvas.height) * 100}%;`;
}

function snapAxis(value: number, candidates: SnapCandidate[], options: SnapOptions) {
  const gridSize = Math.max(1, Math.round(Number(options.gridSize) || 1));
  const threshold = Math.max(0, options.threshold ?? DEFAULT_SNAP_THRESHOLD);
  let snappedValue = Math.round(value / gridSize) * gridSize;
  let guide: number | null = null;
  let bestDistance = threshold + 1;

  for (const candidate of candidates) {
    const distance = Math.abs(value - candidate.position);
    if (distance <= threshold && distance < bestDistance) {
      snappedValue = candidate.position;
      guide = candidate.guide;
      bestDistance = distance;
    }
  }

  return { value: snappedValue, guide };
}

function xCandidates(item: SnapItem, canvas: SnapCanvas, items: SnapItem[]) {
  const candidates: SnapCandidate[] = [
    { position: 0, guide: 0 },
    { position: canvas.width / 2 - item.width / 2, guide: canvas.width / 2 },
    { position: canvas.width - item.width, guide: canvas.width }
  ];

  for (const other of items) {
    if (other.id === item.id) continue;
    candidates.push(
      { position: other.x, guide: other.x },
      { position: other.x + other.width - item.width, guide: other.x + other.width },
      { position: other.x + other.width / 2 - item.width / 2, guide: other.x + other.width / 2 }
    );
  }

  return candidates;
}

function yCandidates(item: SnapItem, canvas: SnapCanvas, items: SnapItem[]) {
  const candidates: SnapCandidate[] = [
    { position: 0, guide: 0 },
    { position: canvas.height / 2 - item.height / 2, guide: canvas.height / 2 },
    { position: canvas.height - item.height, guide: canvas.height }
  ];

  for (const other of items) {
    if (other.id === item.id) continue;
    candidates.push(
      { position: other.y, guide: other.y },
      { position: other.y + other.height - item.height, guide: other.y + other.height },
      { position: other.y + other.height / 2 - item.height / 2, guide: other.y + other.height / 2 }
    );
  }

  return candidates;
}

function xResizeCandidates(item: SnapItem, canvas: SnapCanvas, items: SnapItem[]) {
  const candidates: SnapCandidate[] = [
    { position: 0, guide: 0 },
    { position: canvas.width / 2, guide: canvas.width / 2 },
    { position: canvas.width, guide: canvas.width }
  ];

  for (const other of items) {
    if (other.id === item.id) continue;
    candidates.push(
      { position: other.x, guide: other.x },
      { position: other.x + other.width, guide: other.x + other.width },
      { position: other.x + other.width / 2, guide: other.x + other.width / 2 }
    );
  }

  return candidates.filter((candidate) => candidate.position > item.x);
}

function yResizeCandidates(item: SnapItem, canvas: SnapCanvas, items: SnapItem[]) {
  const candidates: SnapCandidate[] = [
    { position: 0, guide: 0 },
    { position: canvas.height / 2, guide: canvas.height / 2 },
    { position: canvas.height, guide: canvas.height }
  ];

  for (const other of items) {
    if (other.id === item.id) continue;
    candidates.push(
      { position: other.y, guide: other.y },
      { position: other.y + other.height, guide: other.y + other.height },
      { position: other.y + other.height / 2, guide: other.y + other.height / 2 }
    );
  }

  return candidates.filter((candidate) => candidate.position > item.y);
}
