import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params, url }) => {
  return {
    layoutId: params.layoutId,
    returnTo: url.searchParams.get("returnTo") ?? "/plugins",
    scrollY: Number.parseInt(url.searchParams.get("scrollY") ?? "0", 10) || 0
  };
};
