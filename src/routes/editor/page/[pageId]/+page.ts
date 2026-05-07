import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params, url }) => {
  return {
    pageId: params.pageId,
    returnTo: url.searchParams.get("returnTo") ?? "/pages",
    scrollY: Number.parseInt(url.searchParams.get("scrollY") ?? "0", 10) || 0
  };
};
