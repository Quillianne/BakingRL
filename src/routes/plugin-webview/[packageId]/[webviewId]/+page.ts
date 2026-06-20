import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params }) => {
  return {
    packageId: params.packageId,
    webviewId: params.webviewId
  };
};
