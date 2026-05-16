type PreserveScrollOptions = {
  key: string | null | undefined;
};

const savedScrollTops = new Map<string, number>();

function optionKey(options: PreserveScrollOptions | string | null | undefined) {
  if (typeof options === "string") return options;
  return options?.key ?? "";
}

function scrollMax(node: HTMLElement) {
  return Math.max(0, node.scrollHeight - node.clientHeight);
}

function frame(callback: () => void) {
  if (typeof requestAnimationFrame === "function") return requestAnimationFrame(callback);
  callback();
  return null;
}

function cancelFrame(handle: number | null) {
  if (handle !== null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(handle);
}

export function preserveScroll(node: HTMLElement, options: PreserveScrollOptions | string | null | undefined) {
  let key = optionKey(options);
  let restoreFrame: number | null = null;
  let lastKnownTop = node.scrollTop;

  function save() {
    if (!key) return;
    savedScrollTops.set(key, lastKnownTop);
  }

  function remember() {
    lastKnownTop = node.scrollTop;
    save();
  }

  function restore(resetIfMissing: boolean) {
    if (!key) return;
    const savedTop = savedScrollTops.get(key);
    if (savedTop === undefined && !resetIfMissing) return;
    node.scrollTop = Math.min(savedTop ?? 0, scrollMax(node));
    lastKnownTop = node.scrollTop;
  }

  function scheduleRestore(resetIfMissing = false) {
    cancelFrame(restoreFrame);
    restoreFrame = frame(() => {
      restoreFrame = null;
      restore(resetIfMissing);
    });
  }

  const mutationObserver =
    typeof MutationObserver === "function"
      ? new MutationObserver(() => scheduleRestore(false))
      : null;
  const resizeObserver =
    typeof ResizeObserver === "function"
      ? new ResizeObserver(() => scheduleRestore(false))
      : null;

  node.addEventListener("scroll", remember, { passive: true });
  mutationObserver?.observe(node, { childList: true, subtree: true });
  resizeObserver?.observe(node);
  scheduleRestore(true);

  return {
    update(nextOptions: PreserveScrollOptions | string | null | undefined) {
      save();
      const nextKey = optionKey(nextOptions);
      const changed = nextKey !== key;
      key = nextKey;
      scheduleRestore(changed);
    },
    destroy() {
      save();
      cancelFrame(restoreFrame);
      mutationObserver?.disconnect();
      resizeObserver?.disconnect();
      node.removeEventListener("scroll", remember);
    }
  };
}
