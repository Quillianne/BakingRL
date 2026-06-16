import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params, url }) => {
  return {
    packageId: params.packageId,
    webviewId: params.webviewId,
    entry: url.searchParams.get("entry") ?? "",
    path: url.searchParams.get("path") ?? ""
  };
};
