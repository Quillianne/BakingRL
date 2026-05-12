export type EditorCanvasModel = {
  width: number;
  height: number;
};

export type EditorItemModel = {
  id: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  z_index: number;
  visible: boolean;
  locked: boolean;
  opacity: number;
};

export type EditorLayerModel<TItem extends EditorItemModel = EditorItemModel> = {
  id: string;
  name: string;
  kind: string;
  visible: boolean;
  locked: boolean;
  order: number;
  items: TItem[];
};

export type EditorItemEntry<
  TItem extends EditorItemModel = EditorItemModel,
  TLayer extends EditorLayerModel<TItem> = EditorLayerModel<TItem>
> = {
  layer: TLayer;
  item: TItem;
};

export type PlacementPoint = {
  x: number;
  y: number;
};

export type ItemDropPosition = "before" | "after" | "end";

export function asWholeNumber(value: unknown, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? Math.round(parsed) : fallback;
}

export function itemStyle(item: EditorItemModel, canvas: EditorCanvasModel | null) {
  if (!canvas) return "";
  return `
    left:${(item.x / canvas.width) * 100}%;
    top:${(item.y / canvas.height) * 100}%;
    width:${(item.width / canvas.width) * 100}%;
    height:${(item.height / canvas.height) * 100}%;
    z-index:${item.z_index};
  `;
}

export function itemContainsPoint(item: EditorItemModel, point: PlacementPoint) {
  return (
    point.x >= item.x &&
    point.x <= item.x + item.width &&
    point.y >= item.y &&
    point.y <= item.y + item.height
  );
}

function layerKindPriority(layer: EditorLayerModel) {
  return layer.kind === "event" ? 1 : 0;
}

export function sortItemEntriesFrontToBack<
  TLayer extends EditorLayerModel<TItem>,
  TItem extends EditorItemModel
>(entries: EditorItemEntry<TItem, TLayer>[]) {
  return [...entries].sort((a, b) => {
    const kindPriority = layerKindPriority(b.layer) - layerKindPriority(a.layer);
    if (kindPriority) return kindPriority;
    const zPriority = b.item.z_index - a.item.z_index;
    if (zPriority) return zPriority;
    const layerPriority = b.layer.order - a.layer.order;
    if (layerPriority) return layerPriority;
    return b.item.id.localeCompare(a.item.id);
  });
}

export function hitTestItemEntries<
  TLayer extends EditorLayerModel<TItem>,
  TItem extends EditorItemModel
>(entries: EditorItemEntry<TItem, TLayer>[], point: PlacementPoint) {
  return sortItemEntriesFrontToBack(
    entries.filter((entry) => entry.layer.visible !== false && entry.item.visible !== false && itemContainsPoint(entry.item, point))
  );
}

export function clampItemToCanvas<TItem extends EditorItemModel>(
  item: TItem,
  canvas: EditorCanvasModel,
  minWidth = 24,
  minHeight = 18
) {
  item.width = Math.round(Math.max(minWidth, Math.min(item.width, canvas.width)));
  item.height = Math.round(Math.max(minHeight, Math.min(item.height, canvas.height)));
  item.x = Math.round(Math.max(0, Math.min(item.x, canvas.width - item.width)));
  item.y = Math.round(Math.max(0, Math.min(item.y, canvas.height - item.height)));
}

export function allLayerItems<
  TLayer extends EditorLayerModel<TItem>,
  TItem extends EditorItemModel
>(layers: TLayer[]) {
  return layers.flatMap((layer) => layer.items.map((item) => ({ layer, item })));
}

export function sortItemsForDisplay<TItem extends EditorItemModel>(items: TItem[]) {
  return [...items].sort((a, b) => b.z_index - a.z_index || a.name.localeCompare(b.name));
}

export function nextZIndex(items: EditorItemModel[]) {
  return Math.max(0, ...items.map((item) => item.z_index)) + 1;
}

export function moveItemByZIndex<TItem extends EditorItemModel>(
  item: TItem,
  items: TItem[],
  direction: -1 | 1
) {
  const ordered = [...items].sort((a, b) => a.z_index - b.z_index || a.id.localeCompare(b.id));
  const index = ordered.findIndex((entry) => entry.id === item.id);
  const targetIndex = index + direction;
  if (index < 0 || targetIndex < 0 || targetIndex >= ordered.length) return false;
  [ordered[index], ordered[targetIndex]] = [ordered[targetIndex], ordered[index]];
  ordered.forEach((entry, nextIndex) => {
    entry.z_index = nextIndex + 1;
  });
  return true;
}

export function moveItemToStackEdge<TItem extends EditorItemModel>(
  item: TItem,
  items: TItem[],
  edge: "front" | "back"
) {
  const ordered = [...items].sort((a, b) => a.z_index - b.z_index || a.id.localeCompare(b.id));
  if (!ordered.some((entry) => entry.id === item.id)) return false;
  const withoutItem = ordered.filter((entry) => entry.id !== item.id);
  const nextOrder = edge === "front" ? [...withoutItem, item] : [item, ...withoutItem];
  nextOrder.forEach((entry, index) => {
    entry.z_index = index + 1;
  });
  return true;
}

export function resizeItemFromDelta<TItem extends Pick<EditorItemModel, "width" | "height">>(
  item: TItem,
  startItem: Pick<EditorItemModel, "width" | "height">,
  dx: number,
  dy: number,
  preserveAspect: boolean,
  options: {
    minWidth?: number;
    minHeight?: number;
    maxWidth?: number;
    maxHeight?: number;
  } = {}
) {
  let width = startItem.width + dx;
  let height = startItem.height + dy;

  if (preserveAspect && startItem.width > 0 && startItem.height > 0) {
    const aspectRatio = startItem.width / startItem.height;
    const widthScale = Math.abs(width / startItem.width - 1);
    const heightScale = Math.abs(height / startItem.height - 1);
    if (widthScale >= heightScale) {
      height = width / aspectRatio;
    } else {
      width = height * aspectRatio;
    }

    const minScale = Math.max(
      (options.minWidth ?? 0) / startItem.width,
      (options.minHeight ?? 0) / startItem.height
    );
    if (minScale > 0 && (width < (options.minWidth ?? 0) || height < (options.minHeight ?? 0))) {
      width = startItem.width * minScale;
      height = startItem.height * minScale;
    }

    const maxScale = Math.min(
      (options.maxWidth ?? Infinity) / startItem.width,
      (options.maxHeight ?? Infinity) / startItem.height
    );
    if (Number.isFinite(maxScale) && maxScale > 0 && (width > (options.maxWidth ?? Infinity) || height > (options.maxHeight ?? Infinity))) {
      width = startItem.width * maxScale;
      height = startItem.height * maxScale;
    }
  }

  item.width = width;
  item.height = height;
}

function normalizeDisplayOrder<TItem extends EditorItemModel>(items: TItem[]) {
  items.forEach((entry, index) => {
    entry.z_index = items.length - index;
  });
}

export function moveItemToLayer<
  TLayer extends EditorLayerModel<TItem>,
  TItem extends EditorItemModel
>(
  sourceLayer: TLayer,
  item: TItem,
  targetLayer: TLayer,
  targetItem: TItem | null,
  position: ItemDropPosition
) {
  const sourceHasItem = sourceLayer.items.some((entry) => entry.id === item.id);
  if (!sourceHasItem) return false;
  if (sourceLayer.id === targetLayer.id && targetItem?.id === item.id) return false;
  if (targetItem && !targetLayer.items.some((entry) => entry.id === targetItem.id)) return false;

  sourceLayer.items = sourceLayer.items.filter((entry) => entry.id !== item.id);
  const targetOrder = sortItemsForDisplay(targetLayer.items.filter((entry) => entry.id !== item.id));
  let insertIndex = targetOrder.length;

  if (targetItem && position !== "end") {
    const targetIndex = targetOrder.findIndex((entry) => entry.id === targetItem.id);
    if (targetIndex !== -1) insertIndex = position === "after" ? targetIndex + 1 : targetIndex;
  }

  targetOrder.splice(insertIndex, 0, item);
  targetLayer.items = targetOrder;
  normalizeDisplayOrder(targetLayer.items);
  if (sourceLayer.id !== targetLayer.id) normalizeDisplayOrder(sourceLayer.items);
  return true;
}

export function insertionPoint(
  canvas: EditorCanvasModel,
  width: number,
  height: number,
  placement?: PlacementPoint | null
) {
  const rawX = placement ? placement.x - width / 2 : canvas.width / 2 - width / 2;
  const rawY = placement ? placement.y - height / 2 : canvas.height / 2 - height / 2;
  return {
    x: Math.round(Math.max(0, Math.min(rawX, canvas.width - width))),
    y: Math.round(Math.max(0, Math.min(rawY, canvas.height - height)))
  };
}
